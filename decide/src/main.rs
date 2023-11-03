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
    let db_url = match std::env::args().nth(2) {
        Some(addr) => addr,
        None => {
            return Err("Pass in database url as second argument.".into());
        }
    };
    let addr = match addr.parse::<SocketAddr>() {
        Ok(addr) => addr,
        Err(e) => {
            return Err(format!("Invalid URL: {}: {}", addr, e).into());
        }
    };
    let static_path = std::env::var("DECIDE_STATIC_PATH").unwrap_or("static".into());
    let routes = vote::routes(&db_url)
        .await
        .or(rps::routes())
        .or(warp::fs::dir(static_path.clone()))
        .or(warp::fs::file(static_path + "/index.html"))
        .with(log);
    warp::serve(routes).run(addr).await;
    Ok(())
}
