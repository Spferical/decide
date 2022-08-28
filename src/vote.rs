use std::{collections::HashMap, sync::Arc};

use futures_util::{SinkExt, StreamExt};
use rand::Rng;
use tokio::sync::{mpsc, Mutex};
use warp::{
    hyper::Uri,
    ws::{Message, WebSocket},
    Reply,
};

use crate::WebResult;

/// Each websocket connection is a unique player.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct ClientId(u64);

#[derive(Clone, PartialEq, Eq, Hash)]
struct RoomId(String);

impl RoomId {
    fn new_random() -> Self {
        Self(format!("{:x}", rand::thread_rng().gen::<u32>()))
    }
}

impl std::fmt::Display for RoomId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug, serde::Deserialize)]
pub struct VoteItem {
    candidate: usize,
    // Lower is better.
    rank: u64,
}

#[derive(Default)]
struct Room {
    clients: HashMap<ClientId, ClientInfo>,
    choices: Vec<String>,
    votes: HashMap<ClientId, Vec<VoteItem>>,
    voting_finished: bool,
}

struct ClientInfo {
    tx: mpsc::Sender<ClientNotification>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "snake_case")]
enum ClientStatus {
    Connected,
    InvalidRoom,
}

#[derive(serde::Serialize)]
struct VoteView {
    choices: Vec<String>,
    voted: bool,
    num_votes: usize,
    num_players: usize,
    voting_results: Option<String>,
}

/// Data serialized and sent to the client in response to a command or other change in state.
#[derive(serde::Serialize)]
struct ClientNotification {
    status: ClientStatus,
    vote: Option<VoteView>,
}

pub struct VoteState {
    rooms: HashMap<RoomId, Room>,
    next_client_id: u64,
}

impl VoteState {
    pub fn new() -> Self {
        Self {
            rooms: HashMap::new(),
            next_client_id: 1,
        }
    }

    async fn broadcast_state(&self, room_id: &RoomId) {
        let room = self.rooms.get(room_id).unwrap();
        for (client_id, client_info) in room.clients.iter() {
            client_info
                .tx
                .send(ClientNotification {
                    status: ClientStatus::Connected,
                    vote: Some(VoteView {
                        choices: room.choices.clone(),
                        voted: room.votes.get(client_id).is_some(),
                        num_votes: room.votes.len(),
                        voting_results: None,
                        num_players: room.clients.len(),
                    }),
                })
                .await
                // Ignore send errors; player could have dropped.
                .ok();
        }
    }

    fn get_new_client_id(&mut self) -> ClientId {
        self.next_client_id += 1;
        ClientId(self.next_client_id - 1)
    }
}

#[derive(serde::Deserialize)]
pub struct NewVoteForm {
    choices: String,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Command {
    Vote(Vec<VoteItem>),
    Tally,
}

pub async fn start_vote(state: Arc<Mutex<VoteState>>, form: NewVoteForm) -> WebResult<impl Reply> {
    let choices = form
        .choices
        .split('\n')
        .map(|choice| choice.trim())
        .filter(|choice| !choice.is_empty())
        .map(|choice| choice.to_owned())
        .collect();
    let room_id = RoomId::new_random();
    state.lock().await.rooms.insert(
        room_id.clone(),
        Room {
            choices,
            votes: HashMap::new(),
            clients: HashMap::new(),
            voting_finished: false,
        },
    );
    let uri = Uri::builder()
        .path_and_query(format!("vote.html?room={room_id}"))
        .build()
        .unwrap();
    Ok(warp::redirect::see_other(uri))
}

pub async fn handle_vote_client(
    global_state: Arc<Mutex<VoteState>>,
    room_id: String,
    mut ws: WebSocket,
) {
    let (tx, mut rx) = mpsc::channel(1);
    let room_id = RoomId(room_id);
    let client_id;
    {
        let mut gs = global_state.lock().await;
        client_id = gs.get_new_client_id();
        let room = if let Some(room) = gs.rooms.get_mut(&room_id) {
            room
        } else {
            ws.send(Message::text(
                serde_json::to_string(&ClientNotification {
                    status: ClientStatus::InvalidRoom,
                    vote: None,
                })
                .unwrap(),
            ))
            .await
            .ok();
            return;
        };
        room.clients.insert(client_id, ClientInfo { tx });
        gs.broadcast_state(&room_id).await;
    }
    let on_command = |global_state: Arc<Mutex<VoteState>>, room_id, client_id, command| async move {
        log::debug!("Got command: {:?}", command);
        match command {
            Command::Vote(votes) => {
                let mut gs = global_state.lock().await;
                let room = gs.rooms.get_mut(&room_id).unwrap();
                room.votes.insert(client_id, votes);
                gs.broadcast_state(&room_id).await;
            }
            Command::Tally => {
                let mut gs = global_state.lock().await;
                let room = gs.rooms.get_mut(&room_id).unwrap();
                room.voting_finished = true;
            }
        }
    };
    loop {
        tokio::select! {
            msg = rx.recv() => match msg {
                Some(msg) => {
                    let msg = serde_json::to_string(&msg).unwrap();
                    if let Err(err) = ws.send(Message::text(msg)).await {
                        log::debug!("Error sending message to client: {}", err);
                        break
                    }
                },
                // server must be shutting down
                None => break,
            },
            item = ws.next() => match item {
                Some(Ok(msg)) => {
                    log::debug!("Got message: {:?}", msg);
                    if msg.is_ping() {
                        if let Err(_) = ws.send(Message::pong("")).await {
                            break;
                        }
                    }
                    match msg.to_str() {
                        Ok(msg) => match serde_json::from_str::<Command>(msg) {
                            Ok(command) => {
                                on_command(
                                    global_state.clone(),
                                    room_id.clone(),
                                    client_id,
                                    command
                                )
                                .await;
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
                None => {
                    log::debug!("Client disconnected.");
                    break
                },
            }
        }
    }
    {
        // cleanup
        let mut gs = global_state.lock().await;
        let room = gs.rooms.get_mut(&room_id).unwrap();
        room.clients.remove(&client_id);
        room.votes.remove(&client_id);
        if room.clients.is_empty() {
            gs.rooms.remove(&room_id);
        } else {
            gs.broadcast_state(&room_id).await;
        }
    }
}
