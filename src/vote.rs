use std::{collections::HashMap, sync::Arc};

use rand::Rng;
use tokio::sync::Mutex;
use warp::{hyper::Uri, Reply};

use crate::WebResult;

/// Each websocket connection is a unique player.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct ClientId(u64);

#[derive(Clone, PartialEq, Eq, Hash)]
struct RoomId(String);

impl RoomId {
    fn new_random() -> Self {
        Self(format!("{:x}", rand::thread_rng().gen::<u32>()))
    }
}

impl std::fmt::Display for RoomId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

struct VoteItem {
    candidate: usize,
    rank: u64,
}

#[derive(Default)]
struct Room {
    choices: Vec<String>,
    votes: HashMap<ClientId, Vec<VoteItem>>,
}

pub struct VoteState {
    rooms: HashMap<RoomId, Room>,
    next_client_id: u64,
}

impl VoteState {
    pub fn new() -> Self {
        Self {
            rooms: HashMap::new(),
            next_client_id: 1,
        }
    }
}

#[derive(serde::Deserialize)]
pub struct NewVoteForm {
    choices: String,
}

pub async fn start_vote(state: Arc<Mutex<VoteState>>, form: NewVoteForm) -> WebResult<impl Reply> {
    let choices = form
        .choices
        .split(' ')
        .map(|choice| choice.trim())
        .filter(|choice| !choice.is_empty())
        .map(|choice| choice.to_owned())
        .collect();
    let room_id = RoomId::new_random();
    state.lock().await.rooms.insert(
        room_id.clone(),
        Room {
            choices,
            votes: HashMap::new(),
        },
    );
    let uri = Uri::builder()
        .path_and_query(format!("vote.html?room={room_id}"))
        .build()
        .unwrap();
    Ok(warp::redirect::see_other(uri))
}
