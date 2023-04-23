use std::{collections::HashMap, sync::Arc};

use futures_util::{sink::SinkExt, stream::StreamExt};
use tokio::sync::{watch, Mutex};
use warp::{
    ws::{Message, WebSocket},
    Filter,
};

use crate::WebResult;

/// Each websocket connection is a unique player.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct ClientId(u64);

#[derive(Clone, PartialEq, Eq, Hash)]
struct RoomId(String);

#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum Choice {
    Rock,
    Paper,
    Scissors,
}

#[derive(Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
enum GameOutcome {
    Win,
    Loss,
    Draw,
}

impl Choice {
    fn get_outcome(self, other: Self) -> GameOutcome {
        match (self, other) {
            (Self::Rock, Self::Paper) => GameOutcome::Loss,
            (Self::Paper, Self::Scissors) => GameOutcome::Loss,
            (Self::Scissors, Self::Rock) => GameOutcome::Loss,
            (Self::Paper, Self::Rock) => GameOutcome::Win,
            (Self::Scissors, Self::Paper) => GameOutcome::Win,
            (Self::Rock, Self::Scissors) => GameOutcome::Win,
            _ => GameOutcome::Draw,
        }
    }
}

struct PlayerState {
    choice: Option<Choice>,
}

struct ClientInfo {
    tx: watch::Sender<Option<ClientNotification>>,
}

#[derive(Default)]
struct Room {
    clients: HashMap<ClientId, ClientInfo>,
    players: HashMap<ClientId, PlayerState>,
    history: Vec<HashMap<ClientId, Choice>>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "snake_case")]
enum ClientStatus {
    Connected,
}

/// Data serialized and sent to the client in response to a command or other change in state.
#[derive(serde::Serialize)]
struct ClientNotification {
    status: ClientStatus,
    room_state: Option<RoomView>,
}

// State sent only to players.
#[derive(serde::Serialize)]
struct PlayerView {
    choice: Option<Choice>,
    opponent_chosen: bool,
    outcome_history: Vec<GameOutcome>,
    wins: u64,
    draws: u64,
    losses: u64,
}

// State sent only to spectators.
#[derive(serde::Serialize)]
struct SpectatorView {
    player_wins: Vec<u64>,
    player_chosen: Vec<bool>,
    draws: u64,
}

/// Representation of room state sent to a client.
#[derive(serde::Serialize)]
struct RoomView {
    num_players: u64,
    num_spectators: u64,
    history: Vec<Vec<Choice>>,
    player_view: Option<PlayerView>,
    spectator_view: Option<SpectatorView>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum Command {
    Choice(Choice),
}

struct RpsState {
    rooms: HashMap<RoomId, Room>,
    next_client_id: u64,
}

impl RpsState {
    pub fn new() -> Self {
        Self {
            rooms: HashMap::new(),
            next_client_id: 1,
        }
    }

    fn get_new_client_id(&mut self) -> ClientId {
        self.next_client_id += 1;
        ClientId(self.next_client_id - 1)
    }

    fn get_game_view(&self, room_id: &RoomId, client_id: ClientId) -> RoomView {
        let room = self.rooms.get(room_id).unwrap();
        let history: Vec<Vec<Choice>> = room
            .history
            .iter()
            .cloned()
            .map(|choices| {
                let mut choices = choices.into_iter().collect::<Vec<_>>();
                // If client is a player, sort his choices first.
                choices.sort_by_key(|(id, _)| if *id == client_id { 0 } else { id.0 + 1 });
                choices.into_iter().map(|(_id, choice)| choice).collect()
            })
            .collect();
        let player_view = room.players.get(&client_id).map(|player_state| {
            let outcome_history = history
                .iter()
                .map(|choices| choices[0].get_outcome(choices[1]))
                .collect();
            let opponent_chosen = room
                .players
                .iter()
                .any(|(id, p)| *id != client_id && p.choice.is_some());
            let (wins, losses, draws) = history.iter().fold((0, 0, 0), |(w, l, d), choices| {
                match choices[0].get_outcome(choices[1]) {
                    GameOutcome::Win => (w + 1, l, d),
                    GameOutcome::Loss => (w, l + 1, d),
                    GameOutcome::Draw => (w, l, d + 1),
                }
            });
            PlayerView {
                choice: player_state.choice,
                opponent_chosen,
                outcome_history,
                wins,
                losses,
                draws,
            }
        });
        let spectator_view = if room.players.contains_key(&client_id) {
            None
        } else {
            let (p1_wins, p2_wins, draws) = history.iter().fold(
                (0, 0, 0),
                |(p1_wins, p2_wins, draws), choices| match choices[0].get_outcome(choices[1]) {
                    GameOutcome::Win => (p1_wins + 1, p2_wins, draws),
                    GameOutcome::Loss => (p1_wins, p2_wins + 1, draws),
                    GameOutcome::Draw => (p1_wins, p2_wins, draws + 1),
                },
            );
            let player_chosen = room.players.values().map(|p| p.choice.is_some()).collect();
            Some(SpectatorView {
                player_wins: vec![p1_wins, p2_wins],
                player_chosen,
                draws,
            })
        };
        let num_players = room.players.len() as u64;
        let num_spectators = room.clients.len() as u64 - num_players;
        RoomView {
            num_players,
            num_spectators,
            history,
            player_view,
            spectator_view,
        }
    }

    async fn broadcast_state(&self, room_id: &RoomId) {
        let room = self.rooms.get(room_id).unwrap();
        for (client_id, client_info) in room.clients.iter() {
            let view = self.get_game_view(room_id, *client_id);
            client_info
                .tx
                .send(Some(ClientNotification {
                    room_state: Some(view),
                    status: ClientStatus::Connected,
                }))
                // Ignore send errors; player could have dropped.
                .ok();
        }
    }
}

async fn handle_rps_client(global_state: Arc<Mutex<RpsState>>, room_id: String, mut ws: WebSocket) {
    let (tx, mut rx) = watch::channel(None);
    let room_id = RoomId(room_id);
    let client_id;
    {
        let mut gs = global_state.lock().await;
        client_id = gs.get_new_client_id();
        let room = gs.rooms.entry(room_id.clone()).or_default();
        room.clients.insert(client_id, ClientInfo { tx });
        if room.players.len() < 2 {
            room.players.insert(client_id, PlayerState { choice: None });
            room.history.clear();
        }
        gs.broadcast_state(&room_id).await;
    }
    let on_command = |global_state: Arc<Mutex<RpsState>>, room_id, client_id, command| {
        async move {
            match command {
                Command::Choice(choice) => {
                    log::debug!("Player {client_id:?} chose {choice:?}");
                    let mut gs = global_state.lock().await;
                    let room = gs.rooms.get_mut(&room_id).unwrap();
                    match room.players.get_mut(&client_id) {
                        Some(mut player_info) => player_info.choice = Some(choice),
                        None => return,
                    }
                    let choices = room
                        .players
                        .iter()
                        .map(|(id, state)| (id, state.choice))
                        .collect::<Vec<_>>();
                    log::debug!("Choices: {choices:?}");
                    if choices.len() == 2 && choices.iter().all(|(_id, choice)| choice.is_some()) {
                        log::debug!("Everyone chose");
                        room.history.push(
                            choices
                                .iter()
                                .map(|(id, choice)| (**id, choice.unwrap()))
                                .collect(),
                        );
                        // clear choices
                        for (_client_id, mut player_state) in room.players.iter_mut() {
                            player_state.choice = None;
                        }
                    }
                    gs.broadcast_state(&room_id).await;
                }
            }
        }
    };
    loop {
        tokio::select! {
            msg = rx.changed() => match msg {
                Ok(()) => {
                    let serialized_msg = {
                        let borrowed_msg = rx.borrow_and_update();
                        serde_json::to_string(borrowed_msg.as_ref().unwrap()).unwrap()
                    };
                    if let Err(err) = ws.send(Message::text(serialized_msg)).await {
                        log::debug!("Error sending message to client: {}", err);
                        break
                    }
                },
                // server must be shutting down
                Err(_) => break,
            },
            item = ws.next() => match item {
                Some(Ok(msg)) => {
                    log::debug!("Got message: {:?}", msg);
                    if msg.is_ping() && ws.send(Message::pong("")).await.is_err() {
                        break;
                    }
                    match msg.to_str() {
                        Ok(msg) => match serde_json::from_str(msg) {
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
        room.players.remove(&client_id);
        room.clients.remove(&client_id);
        if room.clients.is_empty() {
            gs.rooms.remove(&room_id);
        } else {
            gs.broadcast_state(&room_id).await;
        }
    }
}

#[allow(opaque_hidden_inferred_bound)]
pub fn routes() -> impl warp::Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
    let rps_state = Arc::new(Mutex::new(RpsState::new()));
    let with_rps_state = warp::any().map(move || rps_state.clone());
    let rps_route = warp::path!("api" / "rps" / String)
        .and(warp::ws())
        .and(with_rps_state)
        .and_then(|room_id, ws: warp::ws::Ws, rooms| async move {
            WebResult::Ok(ws.on_upgrade(|ws| handle_rps_client(rooms, room_id, ws)))
        });
    rps_route
}
