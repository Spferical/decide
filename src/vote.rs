use std::collections::HashMap;

use warp::{hyper::Uri, Reply};

use crate::WebResult;

/// Each websocket connection is a unique player.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct ClientId(u64);

#[derive(Clone, PartialEq, Eq, Hash)]
struct RoomId(String);

#[derive(Default)]
struct Room {}

pub struct RpsState {
    rooms: HashMap<RoomId, Room>,
    next_client_id: u64,
}

#[derive(serde::Deserialize)]
pub struct NewVoteForm {
    choices: String,
}

pub async fn start_vote(form: NewVoteForm) -> WebResult<impl Reply> {
    let uri: Uri = "vote.html?room=asdfasdf".parse().unwrap();
    Ok(warp::redirect::see_other(uri))
}
