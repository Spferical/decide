use std::{
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Instant,
};

use futures_util::{sink::SinkExt, stream::StreamExt};
use rand::{seq::SliceRandom, Rng};
use tokio::net::TcpStream;
use tokio_tungstenite::{tungstenite::Message, MaybeTlsStream, WebSocketStream};
use uuid::Uuid;

use decide_api as api;

type ClientStream = WebSocketStream<MaybeTlsStream<TcpStream>>;

async fn make_vote(client: &mut reqwest::Client, base_url: &str, secure: bool) -> String {
    let proto = if secure { "https" } else { "http" };
    let url = format!("{proto}://{base_url}/api/start_vote");
    let body = "choices=a%0D%0Ab%0D%0Ac";
    let response = client.post(url).body(body).send().await.unwrap();
    response.url().path().split('/').last().unwrap().into()
}

struct VoteClient {
    ctrl_channel: tokio::sync::mpsc::Sender<ClientReq>,
}

#[derive(Debug)]
enum VoteClientError {
    Disconnected,
}

impl VoteClient {
    #[allow(unused)]
    async fn get_status(&self) -> Result<Arc<api::ClientNotification>, VoteClientError> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.ctrl_channel
            .send(ClientReq::GetStatus(tx))
            .await
            .map_err(|_| VoteClientError::Disconnected)?;
        rx.await.map_err(|_| VoteClientError::Disconnected)
    }

    async fn vote_randomly(&self) -> Result<(), VoteClientError> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.ctrl_channel
            .send(ClientReq::VoteRandomly(tx))
            .await
            .map_err(|_| VoteClientError::Disconnected)?;
        rx.await.map_err(|_| VoteClientError::Disconnected)
    }
    async fn tally(&self) -> Result<(), VoteClientError> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.ctrl_channel
            .send(ClientReq::Tally(tx))
            .await
            .map_err(|_| VoteClientError::Disconnected)?;
        rx.await.map_err(|_| VoteClientError::Disconnected)
    }
}

#[derive(Debug)]
enum ClientReq {
    GetStatus(tokio::sync::oneshot::Sender<Arc<api::ClientNotification>>),
    VoteRandomly(tokio::sync::oneshot::Sender<()>),
    Tally(tokio::sync::oneshot::Sender<()>),
}

async fn client_work(
    init_state: api::ClientNotification,
    mut ws: ClientStream,
    mut req_rx: tokio::sync::mpsc::Receiver<ClientReq>,
) {
    let mut status: Arc<api::ClientNotification> = Arc::new(init_state);
    let mut waiting_on_status = vec![];
    loop {
        tokio::select! {
            req = req_rx.recv() => {
                match req {
                    Some(ClientReq::GetStatus(tx)) => {
                        tx.send(status.clone()).ok();
                    }
                    Some(ClientReq::VoteRandomly(tx)) => {
                        let num_candidates = status
                            .vote
                            .as_ref()
                            .expect("No vote active")
                            .choices
                            .len();
                        let command = {
                            let mut rng = rand::thread_rng();
                            let vote: Vec<api::VoteItem> = (0..num_candidates)
                                .map(|candidate| api::VoteItem {
                                    candidate,
                                    rank: rng.gen_range(0..num_candidates as u64),
                                })
                                .collect();
                            let name = String::from(
                                *["Fred", "Joe", "???"].choose(&mut rng).unwrap());
                            api::Command::Vote(api::UserVote{name, selections: vote})
                        };
                        let command = serde_json::to_string(&command)
                            .expect("Failed to serialize command");
                        // Note: if websocket closes, it should be caught on the receive side.
                        ws.send(Message::Text(command)).await.ok();
                        waiting_on_status.push(tx);
                    }
                    Some(ClientReq::Tally(tx)) => {
                        let command = api::Command::Tally;
                        let command = serde_json::to_string(&command)
                            .expect("Failed to serialize command");
                        ws.send(Message::Text(command)).await.ok();
                        if status.vote.as_ref().filter(|v| v.results.is_some()).is_some() {
                            // If results are already in, we might not get a response.
                            tx.send(()).ok();
                        } else {
                            waiting_on_status.push(tx);
                        }
                    }
                    None => {
                        break;
                    }
                }
            }
            msg = ws.next() => {
                match msg {
                    Some(Ok(Message::Text(txt))) => {
                        let notification: api::ClientNotification =
                            serde_json::from_str(&txt).unwrap();
                        // Errors can only happen on initial sync.
                        assert!(matches!(notification.status, api::ClientStatus::Connected));
                        status = Arc::new(notification);
                        for tx in waiting_on_status.drain(..) {
                            tx.send(()).ok();
                        }
                    }
                    None => {
                        log::debug!("Websocket connection closed");
                        break
                    },
                    msg => {
                        log::debug!("Got unexpected websocket message: {msg:?}");
                    },
                }
            }
        }
    }
    ws.close(None).await.ok();
}

impl VoteClient {
    async fn connect(base_url: &str, vote_id: &str, secure: bool) -> Self {
        let proto = if secure { "wss" } else { "ws" };
        let client_id = Uuid::new_v4().to_string();
        let url = format!("{proto}://{base_url}/api/vote/{vote_id}?id={client_id}");
        let mut ws = tokio_tungstenite::connect_async(url).await.unwrap().0;
        let (tx, rx) = tokio::sync::mpsc::channel(1);
        // wait on initial sync
        let init_state: api::ClientNotification = loop {
            match ws.next().await {
                Some(Ok(Message::Text(txt))) => {
                    break serde_json::from_str(&txt).expect("Invalid initial sync");
                }
                _ => {}
            }
        };

        if !init_state.vote.is_some() {
            panic!("Got initial state: {init_state:?}");
        }

        tokio::spawn(client_work(init_state, ws, rx));
        VoteClient { ctrl_channel: tx }
    }
}

#[tokio::main]
async fn main() {
    pretty_env_logger::init_timed();
    let start = Instant::now();
    let base_url = std::env::args().nth(1).unwrap();
    let secure = std::env::args()
        .filter(|arg| ["-s", "--secure"].contains(&arg.as_str()))
        .next()
        .is_some();
    let total_requests = Arc::new(AtomicU64::new(0));

    let num_rooms = 100;
    let num_clients_per_room = 10;

    // Create rooms.
    let mut http_client = reqwest::Client::new();
    let mut rooms = vec![];
    for _ in 0..num_rooms {
        rooms.push(make_vote(&mut http_client, &base_url, secure).await);
    }
    let rooms_done_timestamp = Instant::now();
    eprintln!(
        "Created {} rooms in {:?}",
        rooms.len(),
        rooms_done_timestamp - start
    );

    // Create and connect clients to each room.
    let mut client_futs = vec![];
    for room in rooms {
        for _ in 0..num_clients_per_room {
            let base_url1 = base_url.clone();
            let room1 = room.clone();
            client_futs.push(async move { VoteClient::connect(&base_url1, &room1, secure).await });
        }
    }
    let clients = futures::future::join_all(client_futs).await;
    let clients_connected_timestamp = Instant::now();
    eprintln!(
        "Created {} clients in {:?}",
        clients.len(),
        clients_connected_timestamp - rooms_done_timestamp
    );

    // Spawn client work.
    let mut client_tasks = vec![];
    for client in clients.into_iter() {
        let total_requests_clone = Arc::clone(&total_requests);
        client_tasks.push(async move {
            log::debug!("Client running");
            loop {
                client.vote_randomly().await.unwrap();
                if 10_000_u64 < total_requests_clone.fetch_add(1, Ordering::Relaxed) {
                    client.tally().await.unwrap();
                    break;
                }
            }
        });
    }
    let mut client_join_set = tokio::task::JoinSet::new();
    for client_fut in client_tasks.drain(..) {
        client_join_set.spawn(client_fut);
    }

    // Wait until all clients are done.
    let mut joined_clients = 0;
    while let Some(res) = client_join_set.join_next().await {
        joined_clients += 1;
        log::debug!(
            "Joined client {joined_clients}/{}",
            num_rooms * num_clients_per_room
        );
        if let Err(err) = res {
            eprintln!("Thread panicked: {:?}", err);
        }
    }
    let end = Instant::now();
    let total_requests = total_requests.load(Ordering::Relaxed);
    eprintln!(
        "Performed {total_requests} requests in {:?}",
        end - clients_connected_timestamp
    );
}
