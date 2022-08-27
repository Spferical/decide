use std::{net::SocketAddr, sync::Arc};

use tokio::sync::Mutex;
use warp::{Filter, Rejection};

mod rps;

type WebResult<T> = std::result::Result<T, Rejection>;

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
    let global_state = Arc::new(Mutex::new(rps::RpsState::new()));
    let with_global_state = warp::any().map(move || global_state.clone());
    let hello = warp::path!("hello" / String).map(|name| format!("Hello, {}!", name));
    let ws_route = warp::path!("rps" / "ws" / String)
        .and(warp::ws())
        .and(with_global_state)
        .and_then(|room_id, ws: warp::ws::Ws, rooms| async move {
            WebResult::Ok(ws.on_upgrade(|ws| rps::handle_rps_client(rooms, room_id, ws)))
        });
    let routes = hello.or(warp::fs::dir("static")).or(ws_route);
    warp::serve(routes).run(addr).await
}
