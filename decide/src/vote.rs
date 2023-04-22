use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use api::VoteWebsocketQueryParams;
use futures_util::{SinkExt, StreamExt};

use tokio::sync::{watch, Mutex};
use uuid::Uuid;
use warp::{
    hyper::Uri,
    ws::{Message, WebSocket},
    Filter, Reply,
};

use decide_api as api;

use crate::{condorcet::ranked_pairs, WebResult};

use self::{
    db::{Db, DbRoom},
    util::{ClientId, RoomId},
};

pub(crate) mod db;
pub(crate) mod util;

/// Interval at which inactive rooms are deleted.
const CLEANUP_INTERVAL: Duration = Duration::from_secs(60 * 60);

/// State for a room stored in process memory.
/// All room changes are synchronized with a mutable reference to this struct.
/// Note: the database is the source of truth for the room state.
struct ServerRoom {
    // Each client may have multiple tabs open.
    clients: HashMap<ClientId, Vec<ConnectionHandle>>,
    // NOTE: the database tally_calculated flag is the source of truth for
    // whether the results are officially tallied i.e. the vote is done.
    // This field is only used to avoid re-calculating the results.
    results_cache: Option<api::CondorcetTally>,
}

impl ServerRoom {
    fn new() -> Self {
        Self {
            clients: HashMap::new(),
            results_cache: None,
        }
    }

    fn add_client(&mut self, client_id: ClientId, handle: ConnectionHandle, db_room: &DbRoom) {
        self.clients.entry(client_id).or_default().push(handle);
        self.broadcast_room_state(db_room)
    }

    fn update_results_cache(&mut self, db_room: &DbRoom) {
        if db_room.tallied && self.results_cache.is_none() {
            self.results_cache = Some(calculate_room_tally(&db_room.choices, &db_room.votes));
        } else if !db_room.tallied && self.results_cache.is_some() {
            log::error!("Results cache incorrectly populated");
            self.results_cache = None;
        }
    }

    fn broadcast_room_state(&mut self, db_room: &DbRoom) {
        self.update_results_cache(db_room);
        for (client_id, client) in self.clients.iter() {
            for handle in client {
                handle
                    .tx
                    .send(Some(self.get_client_notification(client_id, db_room)))
                    .ok();
            }
        }
    }

    fn get_client_notification(
        &self,
        client_id: &ClientId,
        db_room: &DbRoom,
    ) -> api::ClientNotification {
        let DbRoom {
            choices,
            votes,
            tallied,
        } = db_room;
        if *tallied != self.results_cache.is_some() {
            log::error!(
                "Results cache incorrect. Tallied: {tallied}, cache: {}",
                self.results_cache.is_some()
            );
        }
        api::ClientNotification {
            status: api::ClientStatus::Connected,
            vote: Some(api::VoteView {
                choices: choices.clone(),
                your_vote: votes.get(&client_id).cloned(),
                num_votes: votes.len(),
                num_players: self.clients.len(),
                results: self.results_cache.as_ref().map(|tally| api::VotingResults {
                    tally: tally.clone(),
                    votes: db_room.votes.values().cloned().collect(),
                }),
            }),
        }
    }

    async fn submit_vote(
        &mut self,
        room_id: &RoomId,
        client_id: ClientId,
        vote: api::UserVote,
        db: &Db,
    ) {
        let mut db_room = db.read_room_state(room_id).await.expect("Missing DB room");
        db_room.votes.insert(client_id, vote);
        db.write_room_state(room_id, db_room.clone()).await;
        self.broadcast_room_state(&db_room);
    }

    async fn tally(&mut self, room_id: &RoomId, db: &Db) {
        let mut db_room = db.read_room_state(room_id).await.expect("Missing DB room");
        db_room.tallied = true;
        db.write_room_state(room_id, db_room.clone()).await;
        self.broadcast_room_state(&db_room);
    }
}

/// Handle to the async task managing one client's websocket connection.
struct ConnectionHandle {
    // Note: this option is initially None, but all .changed() values must be Some(_).
    tx: watch::Sender<Option<api::ClientNotification>>,
}

pub struct VoteState {
    rooms: HashMap<RoomId, ServerRoom>,
    db: Db,
}

fn calculate_room_tally(
    choices: &[String],
    votes: &HashMap<ClientId, api::UserVote>,
) -> api::CondorcetTally {
    let num_choices = choices.len();
    let votes: Vec<Vec<crate::condorcet::VoteItem>> = votes
        .values()
        .map(|v| {
            v.selections
                .iter()
                .map(|item| crate::condorcet::VoteItem {
                    candidate: item.candidate,
                    rank: item.rank,
                })
                .collect()
        })
        .collect();
    let results = ranked_pairs(num_choices, votes);
    let results = api::CondorcetTally {
        ranks: results.ranks,
        totals: results.totals,
    };
    results
}

impl VoteState {
    async fn init(db_url: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let db = Db::init(db_url).await?;
        Ok(Self {
            rooms: HashMap::new(),
            db,
        })
    }

    async fn create_room(&self, choices: Vec<String>) -> RoomId {
        self.db.create_room(choices).await
    }

    async fn register_client(
        &mut self,
        room_id: &RoomId,
        client_id: ClientId,
        tx: watch::Sender<Option<api::ClientNotification>>,
    ) -> bool {
        let db_room = match self.db.read_room_state(room_id).await {
            Some(room) => room,
            None => {
                log::error!("client {client_id:?} gave invalid room {room_id}");
                return false;
            }
        };
        self.db.bump_room_activity(room_id).await;
        let room = self
            .rooms
            .entry(room_id.clone())
            .or_insert_with(ServerRoom::new);
        room.add_client(client_id, ConnectionHandle { tx }, &db_room);
        true
    }

    async fn submit_vote(&mut self, room_id: &RoomId, client_id: ClientId, vote: api::UserVote) {
        if let Some(room) = self.rooms.get_mut(room_id) {
            room.submit_vote(room_id, client_id, vote, &self.db).await
        }
    }

    async fn tally(&mut self, room_id: &RoomId) {
        if let Some(room) = self.rooms.get_mut(room_id) {
            room.tally(room_id, &self.db).await
        }
    }

    async fn prune_connection_handles(&mut self, room_id: &RoomId, client_id: ClientId) {
        let mut remove_room = false;
        if let Some(room) = self.rooms.get_mut(room_id) {
            if let Some(client_connections) = room.clients.get_mut(&client_id) {
                client_connections.retain(|conn| !conn.tx.is_closed());
                log::debug!(
                    "Room {room_id} has {} connections left",
                    client_connections.len()
                );
                if client_connections.is_empty() {
                    room.clients.remove(&client_id);
                }
            }
            if room.clients.is_empty() {
                remove_room = true;
            }
            let db_room = self
                .db
                .read_room_state(room_id)
                .await
                .expect("Missing DB room");
            room.broadcast_room_state(&db_room);
        }
        if remove_room {
            self.rooms.remove(room_id);
        }
    }

    async fn cleanup_rooms(&mut self) {
        let start = Instant::now();
        for room_id in self.rooms.keys() {
            self.db.bump_room_activity(room_id).await;
        }
        self.db.cleanup_rooms().await;
        log::info!("Cleaned rooms in {:?}", Instant::now() - start);
    }
}

async fn start_vote(state: Arc<Mutex<VoteState>>, form: api::NewVoteForm) -> WebResult<impl Reply> {
    let choices = form
        .choices
        .split('\n')
        .map(|choice| choice.trim())
        .filter(|choice| !choice.is_empty())
        .map(|choice| choice.to_owned())
        .collect();
    let room_id = state.lock().await.create_room(choices).await;
    let uri = Uri::builder()
        .path_and_query(format!("/vote/{room_id}"))
        .build()
        .unwrap();
    Ok(warp::redirect::see_other(uri))
}

async fn handle_vote_client(
    global_state: Arc<Mutex<VoteState>>,
    params: VoteWebsocketQueryParams,
    room_id: String,
    mut ws: WebSocket,
) {
    let room_id = RoomId(room_id);
    let (tx, mut rx) = watch::channel(None);
    let client_id = ClientId(match Uuid::parse_str(&params.id) {
        Ok(uuid) => uuid,
        Err(err) => {
            log::debug!("Failed to parse client UUID: {:?}: {:?}", params.id, err);
            ws.feed(Message::text(
                serde_json::to_string(&api::ClientNotification {
                    status: api::ClientStatus::InvalidUuid,
                    vote: None,
                })
                .unwrap(),
            ))
            .await
            .ok();
            return;
        }
    });
    {
        let mut gs = global_state.lock().await;
        if !gs.register_client(&room_id, client_id, tx).await {
            log::debug!("client {client_id} gave invalid room {room_id}");
            ws.feed(Message::text(
                serde_json::to_string(&api::ClientNotification {
                    status: api::ClientStatus::InvalidRoom,
                    vote: None,
                })
                .unwrap(),
            ))
            .await
            .ok();
            return;
        }
    };
    log::debug!("client {client_id} connected to room {room_id}");
    let on_command = |global_state: Arc<Mutex<VoteState>>, room_id, client_id, command| async move {
        log::debug!("client {client_id} sent command: {:?}", command);
        match command {
            api::Command::Vote(user_vote) => {
                let mut gs = global_state.lock().await;
                gs.submit_vote(&room_id, client_id, user_vote).await;
            }
            api::Command::Tally => {
                let mut gs = global_state.lock().await;
                gs.tally(&room_id).await;
            }
        }
    };
    loop {
        tokio::select! {
            changed = rx.changed() => match changed {
                Ok(()) => {
                    let serialized_msg = {
                        let borrowed_msg = rx.borrow_and_update();
                        let msg_ref = borrowed_msg.as_ref().expect("Bad state broadcast");
                        serde_json::to_string(msg_ref).unwrap()
                    };
                    log::debug!("Sending message: {:?}", serialized_msg);
                    if let Err(err) = ws.send(Message::text(serialized_msg)).await {
                        log::debug!("Error sending message to client: {}", err);
                        break
                    }
                },
                // Sender was dropped; server must be shutting down.
                Err(_) => break,
            },
            item = ws.next() => match item {
                Some(Ok(msg)) => {
                    log::debug!("Got message: {:?}", msg);
                    if msg.is_ping() && ws.send(Message::pong("")).await.is_err() {
                        break
                    } else if msg.is_close() {
                        break
                    } else if msg.is_pong() {
                        continue
                    }
                    match msg.to_str() {
                        Ok(msg) => match serde_json::from_str::<api::Command>(msg) {
                            Ok(command) => {
                                let command_start = Instant::now();
                                let command_name = api::Command::name(&command);
                                on_command(
                                    global_state.clone(),
                                    room_id.clone(),
                                    client_id,
                                    command
                                )
                                .await;
                                let elapsed = Instant::now() - command_start;
                                log::info!("{client_id} {command_name} {elapsed:?}");
                            },
                            Err(e) => {
                                log::debug!("Bad message: {:?}: {:?}", msg, e);
                                continue;
                            },
                        }
                        Err(()) => {
                            log::debug!("Bad message: {:?}", msg);
                        },
                    }
                },
                Some(Err(err)) => {
                    log::debug!("Error reading client response: {}", err);
                    break
                },
                None => break,
            }
        }
    }
    {
        // cleanup
        let mut gs = global_state.lock().await;
        drop(rx);
        gs.prune_connection_handles(&room_id, client_id).await;
        log::debug!("closed connection from client {client_id}");
    }
}

// Background task that cleans up old rooms.
async fn run_cleanup_task(global_state: Arc<Mutex<VoteState>>) {
    loop {
        tokio::time::sleep(CLEANUP_INTERVAL).await;
        let mut gs = global_state.lock().await;
        gs.cleanup_rooms().await;
    }
}

#[allow(opaque_hidden_inferred_bound)]
pub async fn routes(
    db_url: &str,
) -> impl warp::Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
    let vote_state = Arc::new(Mutex::new(VoteState::init(db_url).await.unwrap()));
    tokio::spawn(run_cleanup_task(vote_state.clone()));
    let with_vote_state = warp::any().map(move || vote_state.clone());
    let new_vote_route = warp::path!("api" / "start_vote")
        .and(warp::post())
        .and(warp::body::content_length_limit(1024 * 32))
        .and(with_vote_state.clone())
        .and(warp::body::form())
        .and_then(start_vote);
    let vote_route = warp::path!("api" / "vote" / String)
        .and(warp::query::query())
        .and(warp::ws())
        .and(with_vote_state)
        .and_then(
            |room_id, params: VoteWebsocketQueryParams, ws: warp::ws::Ws, state| async move {
                WebResult::Ok(ws.on_upgrade(|ws| handle_vote_client(state, params, room_id, ws)))
            },
        );
    new_vote_route.or(vote_route)
}
