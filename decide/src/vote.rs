use std::{collections::HashMap, sync::Arc, time::Instant};

use api::VoteWebsocketQueryParams;
use futures_util::{SinkExt, StreamExt};

use tokio::sync::{watch, Mutex};
use uuid::Uuid;
use warp::{
    hyper::Uri,
    ws::{Message, WebSocket},
    Filter, Reply,
};

use decide_api::{self as api, UserVote};

use crate::{
    condorcet::ranked_pairs,
    vote::db::{Room, RoomOptions, RoomV1},
    WebResult,
};

use self::{
    db::Db,
    util::{ClientId, RoomId},
};

pub(crate) mod db;
pub(crate) mod util;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TallyError {
    Unauthorized,
    IncorrectPassword,
}

/// State for a room stored in process memory.
/// All room changes are synchronized with a mutable reference to this struct.
/// Note: the database is the source of truth for the room state.
struct ServerRoom {
    // Each client may have multiple tabs open.
    clients: HashMap<ClientId, Vec<ConnectionHandle>>,
    // Results of the poll. May not be final.
    results: Option<api::CondorcetTally>,
}

impl ServerRoom {
    fn new() -> Self {
        Self {
            clients: HashMap::new(),
            results: None,
        }
    }

    fn add_client(&mut self, client_id: ClientId, handle: ConnectionHandle, db_room: &Room) {
        self.clients.entry(client_id).or_default().push(handle);
        self.broadcast_room_state(db_room)
    }

    fn update_results_cache(&mut self, room: &Room) {
        match room {
            Room::V1(room) => {
                if room.tallied || (room.live_results && !room.votes.is_empty()) {
                    self.results = Some(calculate_room_tally(&room.choices, &room.votes));
                }
            }
        }
    }

    fn broadcast_room_state(&mut self, db_room: &Room) {
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
        room: &Room,
    ) -> api::ClientNotification {
        match room {
            Room::V1(RoomV1 {
                choices,
                votes,
                tallied,
                live_results,
                creator_id,
                end_rule,
                end_password: _,
            }) => api::ClientNotification {
                status: api::ClientStatus::Connected,
                vote: Some(api::VoteView {
                    choices: choices.clone(),
                    your_vote: votes.get(client_id).cloned(),
                    num_votes: votes.len(),
                    num_players: self.clients.len(),
                    tallied: *tallied,
                    live_results: *live_results,
                    results: self.results.as_ref().map(|tally| api::VotingResults {
                        tally: tally.clone(),
                        votes: votes.values().cloned().collect(),
                    }),
                    end_rule: *end_rule,
                    is_creator: Some(*client_id) == *creator_id,
                }),
            },
        }
    }

    async fn submit_vote(
        &mut self,
        room_id: &RoomId,
        client_id: ClientId,
        vote: api::UserVote,
        db: &Db,
    ) {
        let room = db.read_room_state(room_id).await.expect("Missing DB room");
        match room {
            Room::V1(mut room) => {
                room.votes.insert(client_id, vote);
                db.write_room_state(room_id, Room::V1(room.clone())).await;
                self.broadcast_room_state(&Room::V1(room));
            }
        }
    }

    async fn tally(
        &mut self,
        room_id: &RoomId,
        client_id: ClientId,
        password: Option<String>,
        db: &Db,
    ) -> Result<(), TallyError> {
        let room = db.read_room_state(room_id).await.expect("Missing DB room");
        match room {
            Room::V1(mut room) => {
                match room.end_rule {
                    api::EndRule::Anyone => {}
                    api::EndRule::Creator => {
                        if room.creator_id != Some(client_id) {
                            return Err(TallyError::Unauthorized);
                        }
                    }
                    api::EndRule::Password => {
                        if let Some(ref pwd_hash) = room.end_password {
                            let matches = password
                                .as_ref()
                                .map(|p| bcrypt::verify(p, pwd_hash).unwrap_or(false))
                                .unwrap_or(false);
                            if !matches {
                                return Err(TallyError::IncorrectPassword);
                            }
                        }
                    }
                }
                room.tallied = true;
                db.write_room_state(room_id, Room::V1(room.clone())).await;
                self.broadcast_room_state(&Room::V1(room));
                Ok(())
            }
        }
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
    votes: &HashMap<ClientId, UserVote>,
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

    api::CondorcetTally {
        ranks: results.ranks,
        totals: results.totals,
    }
}

impl VoteState {
    async fn init(db_url: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let db = Db::init(db_url).await?;
        Ok(Self {
            rooms: HashMap::new(),
            db,
        })
    }

    async fn create_room(&self, opts: RoomOptions) -> RoomId {
        self.db.create_room(opts).await
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

    async fn tally(
        &mut self,
        room_id: &RoomId,
        client_id: ClientId,
        password: Option<String>,
    ) -> Result<(), TallyError> {
        if let Some(room) = self.rooms.get_mut(room_id) {
            room.tally(room_id, client_id, password, &self.db).await
        } else {
            Err(TallyError::Unauthorized)
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
}

async fn start_vote(state: Arc<Mutex<VoteState>>, form: api::NewVoteForm) -> WebResult<impl Reply> {
    let choices: Vec<String> = form
        .choices
        .split('\n')
        .map(|choice| choice.trim())
        .filter(|choice| !choice.is_empty())
        .map(|choice| choice.to_owned())
        .collect();
    let live_results = form.live_results;
    let creator_id = Uuid::parse_str(&form.creator_id).ok().map(ClientId);
    let end_rule = form.end_rule;
    let end_password = form
        .end_password
        .filter(|p| !p.is_empty())
        .map(|p| bcrypt::hash(p, bcrypt::DEFAULT_COST).expect("Failed to hash password"));
    let opts = RoomOptions {
        choices,
        live_results,
        creator_id,
        end_rule,
        end_password,
    };
    let room_id = state.lock().await.create_room(opts).await;
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
            let _ = ws.flush().await;
            return;
        }
    };
    log::debug!("client {client_id} connected to room {room_id}");
    let on_command = |global_state: Arc<Mutex<VoteState>>, room_id, client_id, command| async move {
        log::debug!("client {client_id} sent command: {command:?}");
        match command {
            api::Command::Vote(user_vote) => {
                let mut gs = global_state.lock().await;
                gs.submit_vote(&room_id, client_id, user_vote).await;
                None
            }
            api::Command::Tally(password) => {
                let mut gs = global_state.lock().await;
                match gs.tally(&room_id, client_id, password).await {
                    Ok(()) => None,
                    Err(TallyError::IncorrectPassword) => Some(api::ClientStatus::InvalidPassword),
                    Err(TallyError::Unauthorized) => None,
                }
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
                    log::debug!("Sending message: {serialized_msg:?}");
                    if let Err(err) = ws.send(Message::text(serialized_msg)).await {
                        log::debug!("Error sending message to client: {err}");
                        break
                    }
                },
                // Sender was dropped; server must be shutting down.
                Err(_) => break,
            },
            item = ws.next() => match item {
                Some(Ok(msg)) => {
                    log::debug!("Got message: {msg:?}");
                    #[allow(clippy::if_same_then_else)]
                    if msg.is_ping() {
                        if ws.send(Message::pong("")).await.is_err() {
                            break
                        }
                        continue
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
                                if let Some(err_status) = on_command(
                                    global_state.clone(),
                                    room_id.clone(),
                                    client_id,
                                    command
                                )
                                .await {
                                    let err_msg = serde_json::to_string(&api::ClientNotification {
                                        status: err_status,
                                        vote: None,
                                    }).unwrap();
                                    if ws.send(Message::text(err_msg)).await.is_err() {
                                        break;
                                    }
                                }
                                let elapsed = Instant::now() - command_start;
                                log::info!("{client_id} {command_name} {elapsed:?}");
                            },
                            Err(e) => {
                                log::debug!("Bad message: {msg:?}: {e:?}");
                                continue;
                            },
                        }
                        Err(()) => {
                            log::debug!("Bad message: {msg:?}");
                        },
                    }
                },
                Some(Err(err)) => {
                    log::debug!("Error reading client response: {err}");
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

#[allow(opaque_hidden_inferred_bound)]
pub async fn routes(
    db_url: &str,
) -> impl warp::Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
    let vote_state = Arc::new(Mutex::new(VoteState::init(db_url).await.unwrap()));
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
