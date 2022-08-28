use std::{net::SocketAddr, sync::Arc};

use tokio::sync::Mutex;
use warp::{Filter, Rejection};

mod rps;
mod vote;

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
    let rps_state = Arc::new(Mutex::new(rps::RpsState::new()));
    let with_rps_state = warp::any().map(move || rps_state.clone());
    let vote_state = Arc::new(Mutex::new(vote::VoteState::new()));
    let with_vote_state = warp::any().map(move || vote_state.clone());
    let new_vote_route = warp::path!("start_vote")
        .and(warp::post())
        .and(warp::body::content_length_limit(1024 * 32))
        .and(with_vote_state.clone())
        .and(warp::body::form())
        .and_then(vote::start_vote);
    let rps_route = warp::path!("rps" / "ws" / String)
        .and(warp::ws())
        .and(with_rps_state)
        .and_then(|room_id, ws: warp::ws::Ws, rooms| async move {
            WebResult::Ok(ws.on_upgrade(|ws| rps::handle_rps_client(rooms, room_id, ws)))
        });
    let vote_route = warp::path!("vote" / "ws" / String)
        .and(warp::ws())
        .and(with_vote_state)
        .and_then(|room_id, ws: warp::ws::Ws, state| async move {
            WebResult::Ok(ws.on_upgrade(|ws| vote::handle_vote_client(state, room_id, ws)))
        });
    let routes = new_vote_route
        .or(vote_route)
        .or(warp::fs::dir("static"))
        .or(rps_route);
    warp::serve(routes).run(addr).await
}
