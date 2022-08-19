use std::{collections::HashMap, sync::Arc};

use futures_util::{sink::SinkExt, stream::StreamExt};
use tokio::sync::{mpsc, Mutex};
use warp::{
    ws::{Message, WebSocket},
    Filter, Rejection, Reply,
};

type WebResult<T> = std::result::Result<T, Rejection>;

struct PlayerComm {
    to_player: mpsc::Sender<()>,
    from_player: mpsc::Receiver<()>,
}

#[derive(Default)]
struct Game {
    player1: Option<PlayerComm>,
    player2: Option<PlayerComm>,
}

async fn handle_client_connection(
    rooms: Arc<Mutex<HashMap<String, Game>>>,
    id: String,
    mut ws: WebSocket,
) {
    let (tx1, rx1) = mpsc::channel(1);
    let (tx2, rx2) = mpsc::channel(1);
    {
        let mut rooms = rooms.lock().await;
        let game = rooms.entry(id).or_default();
        let comm = PlayerComm {
            to_player: tx1,
            from_player: rx2,
        };
        let player_idx = if game.player1.is_none() {
            game.player1 = Some(comm);
            1
        } else if game.player2.is_none() {
            game.player2 = Some(comm);
            2
        } else {
            eprintln!("Room full!");
            ws.send(Message::text("room full")).await.unwrap();
            return;
        };
    }
    loop {
        tokio::select! {
            item = ws.next() => match item {
                Some(Ok(msg)) => {
                    eprintln!("Got message: {:?}", msg);
                },
                Some(Err(err)) => {
                    eprintln!("Error: {}", err);
                    break
                },
                None => {
                    eprintln!("Client disconnected.");
                    break
                },
            }
        }
    }
}

#[tokio::main]
async fn main() {
    let rooms = Arc::new(Mutex::new(HashMap::<String, Game>::new()));
    let with_rooms = warp::any().map(move || rooms.clone());
    let hello = warp::path!("hello" / String).map(|name| format!("Hello, {}!", name));
    let ws_route = warp::path!(String)
        .and(warp::ws())
        .and(with_rooms)
        .and_then(|room_id, ws: warp::ws::Ws, rooms| async move {
            WebResult::Ok(ws.on_upgrade(|ws| handle_client_connection(rooms, room_id, ws)))
        });
    let routes = hello.or(warp::fs::dir("static")).or(ws_route);
    warp::serve(routes).run(([127, 0, 0, 1], 8000)).await
}
