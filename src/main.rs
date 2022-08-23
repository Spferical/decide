use std::{collections::HashMap, net::SocketAddr, sync::Arc};

use futures_util::{sink::SinkExt, stream::StreamExt};
use tokio::sync::{mpsc, Mutex};
use warp::{
    ws::{Message, WebSocket},
    Filter, Rejection,
};

type WebResult<T> = std::result::Result<T, Rejection>;

/// Each websocket connection is a unique player.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct PlayerId(u64);

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
    tx: mpsc::Sender<ClientNotification>,
    choice: Option<Choice>,
}

#[derive(Default)]
struct Room {
    players: HashMap<PlayerId, PlayerState>,
    history: Vec<HashMap<PlayerId, Choice>>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "snake_case")]
enum ClientStatus {
    Connected,
    RoomFull,
}

/// Data serialized and sent to the client in response to a command or other change in state.
#[derive(serde::Serialize)]
struct ClientNotification {
    status: ClientStatus,
    room_state: Option<RoomView>,
}

#[derive(serde::Serialize)]
struct HistoryEntryView {
    outcome: GameOutcome,
    choices: Vec<Choice>,
}

/// Representation of room state sent to a player.
#[derive(serde::Serialize)]
struct RoomView {
    num_players: u64,
    choice: Option<Choice>,
    opponent_chosen: bool,
    history: Vec<HistoryEntryView>,
    wins: u64,
    draws: u64,
    losses: u64,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum Command {
    Choice(Choice),
}

struct GlobalState {
    rooms: HashMap<RoomId, Room>,
    next_player_id: u64,
}

impl GlobalState {
    fn new() -> Self {
        Self {
            rooms: HashMap::new(),
            next_player_id: 1,
        }
    }

    fn get_new_player_id(&mut self) -> PlayerId {
        self.next_player_id += 1;
        PlayerId(self.next_player_id - 1)
    }

    fn get_game_view(&self, room_id: &RoomId, player_id: PlayerId) -> RoomView {
        let room = self.rooms.get(&room_id).unwrap();
        let history = room
            .history
            .iter()
            .map(|choices| {
                let mut choices = choices.clone();
                let player_choice = choices.remove(&player_id).unwrap();
                // assuming only one other rps player
                let other_choice = *choices.iter().next().unwrap().1;
                let outcome = player_choice.get_outcome(other_choice);
                HistoryEntryView {
                    outcome,
                    choices: vec![player_choice, other_choice],
                }
            })
            .collect::<Vec<_>>();
        let (wins, losses, draws) = history
            .iter()
            .fold((0, 0, 0), |(w, l, d), entry| match entry.outcome {
                GameOutcome::Win => (w + 1, l, d),
                GameOutcome::Loss => (w, l + 1, d),
                GameOutcome::Draw => (w, l, d + 1),
            });
        let opponent_chosen = room
            .players
            .iter()
            .find(|(id, p)| **id != player_id && p.choice.is_some())
            .is_some();
        RoomView {
            num_players: room.players.len() as u64,
            choice: room.players.get(&player_id).unwrap().choice,
            opponent_chosen,
            history,
            wins,
            losses,
            draws,
        }
    }

    async fn send_state_to_players(&self, room_id: &RoomId) {
        let room = self.rooms.get(room_id).unwrap();
        for (player_id, player_state) in room.players.iter() {
            let view = self.get_game_view(&room_id, *player_id);
            player_state
                .tx
                .send(ClientNotification {
                    room_state: Some(view),
                    status: ClientStatus::Connected,
                })
                .await
                // Ignore send errors; player could have dropped.
                .ok();
        }
    }
}

async fn handle_client_connection(
    global_state: Arc<Mutex<GlobalState>>,
    room_id: String,
    mut ws: WebSocket,
) {
    let (tx, mut rx) = mpsc::channel(1);
    let room_id = RoomId(room_id);
    let player_id;
    {
        let mut gs = global_state.lock().await;
        player_id = gs.get_new_player_id();
        let room = gs.rooms.entry(room_id.clone()).or_default();
        if room.players.len() == 2 {
            // Ignore error as player could have disconnected
            ws.send(Message::text(
                serde_json::to_string(&ClientNotification {
                    status: ClientStatus::RoomFull,
                    room_state: None,
                })
                .unwrap(),
            ))
            .await
            .ok();
            return;
        }
        room.players
            .insert(player_id, PlayerState { tx, choice: None });
        room.history.clear();
        gs.send_state_to_players(&room_id).await;
    }
    let on_command = |global_state: Arc<Mutex<GlobalState>>, room_id, player_id, command| {
        async move {
            match command {
                Command::Choice(choice) => {
                    log::debug!("Player {player_id:?} chose {choice:?}");
                    let mut gs = global_state.lock().await;
                    let room = gs.rooms.get_mut(&room_id).unwrap();
                    room.players.get_mut(&player_id).unwrap().choice = Some(choice);
                    let choices = room
                        .players
                        .iter()
                        .map(|(id, state)| (id, state.choice))
                        .collect::<Vec<_>>();
                    log::debug!("Choices: {choices:?}");
                    if choices.len() < 2 {
                        return;
                    }
                    if choices.iter().all(|(_id, choice)| choice.is_some()) {
                        log::debug!("Everyone chose");
                        room.history.push(
                            choices
                                .iter()
                                .map(|(id, choice)| (**id, choice.unwrap()))
                                .collect(),
                        );
                        // clear choices
                        for (_player_id, mut player_state) in room.players.iter_mut() {
                            player_state.choice = None;
                        }
                    }
                    gs.send_state_to_players(&room_id).await;
                }
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
                        Ok(msg) => match serde_json::from_str(msg) {
                            Ok(command) => {
                                on_command(
                                    global_state.clone(),
                                    room_id.clone(),
                                    player_id,
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
        room.players.remove(&player_id);
        if room.players.is_empty() {
            gs.rooms.remove(&room_id);
        } else {
            gs.send_state_to_players(&room_id).await;
        }
    }
}

#[tokio::main]
async fn main() {
    pretty_env_logger::init_timed();
    let addr = match std::env::args().nth(1) {
        Some(addr) => addr,
        None => {
            log::error!("Pass in server url as first argument.");
            return;
        }
    };
    let addr = match addr.parse::<SocketAddr>() {
        Ok(addr) => addr,
        Err(e) => {
            log::error!("Invalid URL: {}: {}", addr, e);
            return;
        }
    };
    let global_state = Arc::new(Mutex::new(GlobalState::new()));
    let with_global_state = warp::any().map(move || global_state.clone());
    let hello = warp::path!("hello" / String).map(|name| format!("Hello, {}!", name));
    let ws_route = warp::path!("ws" / String)
        .and(warp::ws())
        .and(with_global_state)
        .and_then(|room_id, ws: warp::ws::Ws, rooms| async move {
            WebResult::Ok(ws.on_upgrade(|ws| handle_client_connection(rooms, room_id, ws)))
        });
    let routes = hello.or(warp::fs::dir("static")).or(ws_route);
    warp::serve(routes).run(addr).await
}
