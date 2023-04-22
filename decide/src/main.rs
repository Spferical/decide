use std::net::SocketAddr;

use warp::{Filter, Rejection};

mod condorcet;
mod rps;
mod vote;

type WebResult<T> = std::result::Result<T, Rejection>;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    pretty_env_logger::init_timed();
    let log = warp::log("decide");
    let addr = match std::env::args().nth(1) {
        Some(addr) => addr,
        None => {
            return Err("Pass in server url as first argument.".into());
        }
    };
    let addr = match addr.parse::<SocketAddr>() {
        Ok(addr) => addr,
        Err(e) => {
            return Err(format!("Invalid URL: {}: {}", addr, e).into());
        }
    };
    let routes = vote::routes()
        .await
        .or(rps::routes())
        .or(warp::fs::dir("static"))
        .or(warp::fs::file("static/index.html"))
        .with(log);
    warp::serve(routes).run(addr).await;
    Ok(())
}
