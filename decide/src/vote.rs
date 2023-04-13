use std::{collections::HashMap, hash::Hash, sync::Arc, time::Instant};

use api::VoteWebsocketQueryParams;
use futures_util::{SinkExt, StreamExt};
use rand::distributions::DistString;
use tokio::sync::{watch, Mutex};
use uuid::Uuid;
use warp::{
    hyper::Uri,
    ws::{Message, WebSocket},
    Filter, Reply,
};

use decide_api as api;

use crate::{condorcet::ranked_pairs, WebResult};

/// Each websocket connection is a unique player.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct ClientId(Uuid);

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
    // Each client may have multiple tabs open.
    clients: HashMap<ClientId, Vec<ConnectionHandle>>,
    choices: Vec<String>,
    votes: HashMap<ClientId, api::UserVote>,
    results: Option<api::CondorcetTally>,
}

/// Handle to the async task managing one client's websocket connection.
struct ConnectionHandle {
    // Note: this option is initially None, but all .changed() values must be Some(_).
    tx: watch::Sender<Option<api::ClientNotification>>,
}

struct VoteState {
    rooms: HashMap<RoomId, Room>,
}

impl VoteState {
    fn new() -> Self {
        Self {
            rooms: HashMap::new(),
        }
    }

    async fn create_room(&mut self, choices: Vec<String>) -> RoomId {
        let room_id = RoomId::new_random();
        self.rooms.insert(
            room_id.clone(),
            Room {
                choices,
                votes: HashMap::new(),
                clients: HashMap::new(),
                results: None,
            },
        );
        room_id
    }

    /// Returns false if the room does not exist, else true.
    async fn register_client(
        &mut self,
        room_id: &RoomId,
        client_id: ClientId,
        tx: watch::Sender<Option<api::ClientNotification>>,
    ) -> bool {
        if let Some(room) = self.rooms.get_mut(room_id) {
            room.clients
                .entry(client_id)
                .or_default()
                .push(ConnectionHandle { tx });
            self.broadcast_state(&room_id).await;
            true
        } else {
            false
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
        for (client_id, client_infos) in room.clients.iter() {
            for client_info in client_infos.iter() {
                client_info
                    .tx
                    .send_replace(Some(self.get_client_notification(client_id, room)));
            }
        }
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
        }
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
        let client_connections = room.clients.get_mut(&client_id).unwrap();
        drop(rx);
        client_connections.retain(|conn| !conn.tx.is_closed());
        log::debug!("{} connections left", client_connections.len());
        if client_connections.is_empty() {
            room.clients.remove(&client_id);
        }
        if room.clients.is_empty() {
            gs.rooms.remove(&room_id);
        } else {
            gs.broadcast_state(&room_id).await;
        }
        log::debug!("client {client_id:?} disconnected.");
    }
}

#[allow(opaque_hidden_inferred_bound)]
pub fn routes() -> impl warp::Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
    let vote_state = Arc::new(Mutex::new(VoteState::new()));
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
