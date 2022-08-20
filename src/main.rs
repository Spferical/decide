use std::{collections::HashMap, sync::Arc};

use futures_util::{sink::SinkExt, stream::StreamExt};
use tokio::sync::{mpsc, Mutex};
use warp::{
    ws::{Message, WebSocket},
    Filter, Rejection,
};

type WebResult<T> = std::result::Result<T, Rejection>;

/// Each player represents a single websocket connection.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct PlayerId(u64);

#[derive(Clone, PartialEq, Eq, Hash)]
struct RoomId(String);

#[derive(Default)]
struct Game {
    players: Vec<PlayerId>,
}

/// Representation of game state sent to a player.
#[derive(serde::Serialize)]
struct GameView {
    num_players: u64,
}

struct GlobalState {
    rooms: HashMap<RoomId, Game>,
    player_channels: HashMap<PlayerId, mpsc::Sender<String>>,
    next_player_id: u64,
}

impl GlobalState {
    fn new() -> Self {
        Self {
            rooms: HashMap::new(),
            player_channels: HashMap::new(),
            next_player_id: 1,
        }
    }

    fn get_new_player_id(&mut self) -> PlayerId {
        self.next_player_id += 1;
        PlayerId(self.next_player_id - 1)
    }

    fn get_game_view(&self, room: &RoomId, _id: PlayerId) -> GameView {
        let game = self.rooms.get(&room).unwrap();
        GameView {
            num_players: game.players.len() as u64,
        }
    }

    async fn send_state_to_players(&self, room_id: &RoomId) {
        let room = self.rooms.get(room_id).unwrap();
        for player in room.players.iter() {
            let view = self.get_game_view(&room_id, *player);
            self.player_channels
                .get(player)
                .unwrap()
                .send(serde_json::to_string(&view).unwrap())
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
        gs.player_channels.insert(player_id, tx);
        let room = gs.rooms.entry(room_id.clone()).or_default();
        room.players.push(player_id);
        gs.send_state_to_players(&room_id).await;
    }
    loop {
        tokio::select! {
            msg = rx.recv() => match msg {
                Some(msg) => {
                    if let Err(err) = ws.send(Message::text(msg)).await {
                        eprintln!("Error sending message to client: {}", err);
                        break
                    }
                },
                // server must be shutting down
                None => break,
            },
            item = ws.next() => match item {
                Some(Ok(msg)) => {
                    eprintln!("Got message: {:?}", msg);
                },
                Some(Err(err)) => {
                    eprintln!("Error reading client response: {}", err);
                    break
                },
                None => {
                    eprintln!("Client disconnected.");
                    break
                },
            }
        }
    }
    {
        // cleanup
        let mut gs = global_state.lock().await;
        let room = gs.rooms.get_mut(&room_id).unwrap();
        room.players
            .remove(room.players.iter().position(|x| *x == player_id).unwrap());
        if room.players.is_empty() {
            gs.rooms.remove(&room_id);
        }
    }
}

#[tokio::main]
async fn main() {
    let global_state = Arc::new(Mutex::new(GlobalState::new()));
    let with_global_state = warp::any().map(move || global_state.clone());
    let hello = warp::path!("hello" / String).map(|name| format!("Hello, {}!", name));
    let ws_route = warp::path!(String)
        .and(warp::ws())
        .and(with_global_state)
        .and_then(|room_id, ws: warp::ws::Ws, rooms| async move {
            WebResult::Ok(ws.on_upgrade(|ws| handle_client_connection(rooms, room_id, ws)))
        });
    let routes = hello.or(warp::fs::dir("static")).or(ws_route);
    warp::serve(routes).run(([127, 0, 0, 1], 8000)).await
}
