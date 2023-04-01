use std::{collections::HashMap, hash::Hash, sync::Arc, time::Instant};

use futures_util::{SinkExt, StreamExt};
use rand::distributions::DistString;
use tokio::sync::{watch, Mutex};
use warp::{
    hyper::Uri,
    ws::{Message, WebSocket},
    Reply,
};

use decide_api as api;

use crate::{condorcet::ranked_pairs, WebResult};

/// Each websocket connection is a unique player.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct ClientId(u64);

#[derive(Clone, PartialEq, Eq, Hash)]
struct RoomId(String);

impl RoomId {
    fn new_random() -> Self {
        Self(rand::distributions::Alphanumeric.sample_string(&mut rand::thread_rng(), 8))
    }
}

impl std::fmt::Display for RoomId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Default)]
struct Room {
    clients: HashMap<ClientId, ClientInfo>,
    choices: Vec<String>,
    votes: HashMap<ClientId, api::UserVote>,
    results: Option<api::CondorcetTally>,
}

struct ClientInfo {
    // Note: this option is initially None, but all .changed() values must be Some(_).
    tx: watch::Sender<Option<api::ClientNotification>>,
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

    fn get_client_notification(
        &self,
        client_id: &ClientId,
        room: &Room,
    ) -> api::ClientNotification {
        api::ClientNotification {
            status: api::ClientStatus::Connected,
            vote: Some(api::VoteView {
                choices: room.choices.clone(),
                your_vote: room.votes.get(client_id).cloned(),
                num_votes: room.votes.len(),
                results: room.results.as_ref().map(|tally| api::VotingResults {
                    tally: tally.clone(),
                    votes: room.votes.values().cloned().collect(),
                }),
                num_players: room.clients.len(),
            }),
        }
    }

    async fn broadcast_state(&self, room_id: &RoomId) {
        let room = self.rooms.get(room_id).unwrap();
        for (client_id, client_info) in room.clients.iter() {
            client_info
                .tx
                .send_replace(Some(self.get_client_notification(client_id, room)));
        }
    }

    fn get_new_client_id(&mut self) -> ClientId {
        self.next_client_id += 1;
        ClientId(self.next_client_id - 1)
    }
}

pub async fn start_vote(
    state: Arc<Mutex<VoteState>>,
    form: api::NewVoteForm,
) -> WebResult<impl Reply> {
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
            results: None,
        },
    );
    let uri = Uri::builder()
        .path_and_query(format!("/vote/{room_id}"))
        .build()
        .unwrap();
    Ok(warp::redirect::see_other(uri))
}

pub async fn handle_vote_client(
    global_state: Arc<Mutex<VoteState>>,
    room_id: String,
    mut ws: WebSocket,
) {
    let room_id = RoomId(room_id);
    let client_id;
    let (tx, mut rx) = watch::channel(None);
    {
        let mut gs = global_state.lock().await;
        client_id = gs.get_new_client_id();
        let room = if let Some(room) = gs.rooms.get_mut(&room_id) {
            room
        } else {
            log::debug!("client {client_id:?} gave invalid room {room_id}");
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
        };
        // Note: this notification is immediately clobbered by broadcast_state().
        room.clients.insert(client_id, ClientInfo { tx });
        gs.broadcast_state(&room_id).await;
    };
    log::debug!("client {client_id:?} connected to room {room_id}");
    let on_command = |global_state: Arc<Mutex<VoteState>>, room_id, client_id, command| async move {
        log::debug!("Got command: {:?}", command);
        match command {
            api::Command::Vote(user_vote) => {
                let mut gs = global_state.lock().await;
                let room = gs.rooms.get_mut(&room_id).unwrap();
                if room.results.is_none() {
                    room.votes.insert(client_id, user_vote);
                    gs.broadcast_state(&room_id).await;
                }
            }
            api::Command::Tally => {
                let mut gs = global_state.lock().await;
                let room = gs.rooms.get_mut(&room_id).unwrap();
                if room.results.is_some() {
                    // No need to recalculate.
                    return;
                }
                let num_choices = room.choices.len();
                let votes: Vec<Vec<crate::condorcet::VoteItem>> = room
                    .votes
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
                room.results = Some(results.into());
                log::debug!("Vote results: {:?}", room.results);
                gs.broadcast_state(&room_id).await;
            }
        }
    };
    loop {
        tokio::select! {
            changed = rx.changed() => match changed {
                Ok(()) => {
                    let serialized_msg = {
                        let borrowed_msg = rx.borrow_and_update();
                        serde_json::to_string(borrowed_msg.as_ref().unwrap()).unwrap()
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
                        break;
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
                                log::info!("client_{} {command_name} {elapsed:?}", client_id.0);
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
                    log::debug!("Client {client_id:?} disconnected.");
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
        if room.clients.is_empty() {
            gs.rooms.remove(&room_id);
        } else {
            gs.broadcast_state(&room_id).await;
        }
        log::debug!("client {client_id:?} disconnected.");
    }
}
