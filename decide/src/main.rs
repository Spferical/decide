use std::net::SocketAddr;

use warp::{Filter, Rejection};

mod condorcet;
mod rps;
mod vote;

type WebResult<T> = std::result::Result<T, Rejection>;

#[tokio::main]
async fn main() {
    pretty_env_logger::init_timed();
    let log = warp::log("decide");
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
    let routes = vote::routes()
        .or(rps::routes())
        .or(warp::fs::dir("static"))
        .or(warp::fs::file("static/index.html"))
        .with(log);
    warp::serve(routes).run(addr).await
}
