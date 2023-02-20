use std::{
    collections::{HashMap, HashSet},
    hash::Hash,
    sync::Arc,
};

use futures_util::{SinkExt, StreamExt};
use itertools::Itertools as _;
use rand::Rng;
use tokio::sync::{mpsc, Mutex};
use warp::{
    hyper::Uri,
    ws::{Message, WebSocket},
    Reply,
};

#[derive(Debug, Clone, serde::Serialize)]
struct CondorcetTally {
    /// totals[a][b] contains the number of votes where candidate a beat b.
    totals: Vec<Vec<u64>>,
    // Ranks[0] contains the winner(s), ranks[n] contains the winners if you
    // remove the members of all previous ranks.
    ranks: Vec<Vec<usize>>,
}

/// Check if b is reachable from a in a graph.
fn is_reachable(a: usize, b: usize, graph: &[HashSet<usize>]) -> bool {
    let mut discovered = HashSet::new();
    let mut stack = vec![a];
    while let Some(node) = stack.pop() {
        if discovered.contains(&node) {
            continue;
        }
        if node == b {
            return true;
        }
        discovered.insert(node);
        stack.extend(graph[node].iter());
    }
    false
}

/// Compute the results of an election using the ranked pairs method.
fn ranked_pairs(num_choices: usize, votes: Vec<Vec<VoteItem>>) -> CondorcetTally {
    // See http://ericgorr.net/condorcet/rankedpairs/

    // Compute the pairwise matrix, totals.
    // totals[a][b] = the number of votes ranking candidate a over candidate b.
    let mut totals = vec![vec![0; num_choices]; num_choices];
    for mut vote in votes.into_iter() {
        vote.sort_by_key(|item| item.rank);
        for (i, item) in vote.iter().enumerate() {
            for item2 in vote[i + 1..]
                .iter()
                .skip_while(|item2| item2.rank == item.rank)
            {
                totals[item.candidate][item2.candidate] += 1;
            }
        }
    }

    // Compute the ranked pairs, a sequence of (winner, loser), sorted by:
    // 1. strength of victory (number of votes favoring a over b)
    // 2. margin (difference in voters favoring a vs favoring b)
    let mut defeats = (0..num_choices)
        .into_iter()
        .flat_map(|c| {
            let totals = &totals;
            (0..num_choices)
                .into_iter()
                .filter(move |&c2| totals[c][c2] > totals[c2][c])
                .map(move |c2| (c, c2))
        })
        .collect::<Vec<(usize, usize)>>();
    // Sort stability doesn't matter because we group later.
    defeats.sort_unstable_by_key(|&(c1, c2)| (totals[c1][c2], totals[c1][c2] - totals[c2][c1]));
    defeats.reverse();

    // defeat_graph[a].contains(&b) iff a defeats b.
    let mut defeat_graph = vec![HashSet::new(); num_choices];

    // Defeats are grouped with all equivalent defeats (by strength/margin).
    for (_key, current_defeats) in defeats
        .into_iter()
        .group_by(|&(a, b)| (totals[a][b], totals[b][a]))
        .into_iter()
    {
        // Insert new defeats into the graph.
        let current_defeats = current_defeats.collect::<Vec<(usize, usize)>>();
        for (a, b) in current_defeats.iter().cloned() {
            defeat_graph[a].insert(b);
            let strength = totals[a][b];
            let margin = totals[a][b] - totals[b][a];
            log::trace!("considering {a} defeats {b} s{strength} m{margin}");
        }

        // Remove new defeats that are part of a cycle.
        let defeats_in_cycles: Vec<(usize, usize)> = current_defeats
            .iter()
            .cloned()
            .filter(|&(a, b)| is_reachable(b, a, &defeat_graph))
            .collect();
        for &(a, b) in defeats_in_cycles.iter() {
            defeat_graph[a].remove(&b);
        }
        for &(a, b) in current_defeats.iter() {
            if !defeats_in_cycles.contains(&(a, b)) {
                log::trace!("keeping {a} defeats {b}");
            }
        }
    }

    let mut unranked = (0..num_choices).into_iter().collect::<Vec<_>>();
    let mut ranks = vec![];
    while !unranked.is_empty() {
        // Find winners, i.e. undefeated nodes.
        let winners: Vec<usize> = unranked
            .iter()
            .cloned()
            .filter(|&c| !unranked.iter().any(|&c2| defeat_graph[c2].contains(&c)))
            .collect();
        unranked.retain(|c| !winners.contains(c));
        ranks.push(winners);
    }

    CondorcetTally { totals, ranks }
}

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

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct VoteItem {
    candidate: usize,
    // Lower is better.
    rank: u64,
}

#[derive(Default)]
struct Room {
    clients: HashMap<ClientId, ClientInfo>,
    choices: Vec<String>,
    votes: HashMap<ClientId, UserVote>,
    results: Option<CondorcetTally>,
}

struct ClientInfo {
    tx: mpsc::Sender<ClientNotification>,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "snake_case")]
enum ClientStatus {
    Connected,
    InvalidRoom,
}

#[derive(Debug, serde::Serialize)]
struct VotingResults {
    tally: CondorcetTally,
    votes: Vec<UserVote>,
}

#[derive(Debug, serde::Serialize)]
struct VoteView {
    choices: Vec<String>,
    your_vote: Option<UserVote>,
    num_votes: usize,
    num_players: usize,
    results: Option<VotingResults>,
}

/// Data serialized and sent to the client in response to a command or other change in state.
#[derive(Debug, serde::Serialize)]
struct ClientNotification {
    status: ClientStatus,
    vote: Option<VoteView>,
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

    async fn broadcast_state(&self, room_id: &RoomId) {
        let room = self.rooms.get(room_id).unwrap();
        for (client_id, client_info) in room.clients.iter() {
            client_info
                .tx
                .send(ClientNotification {
                    status: ClientStatus::Connected,
                    vote: Some(VoteView {
                        choices: room.choices.clone(),
                        your_vote: room.votes.get(client_id).cloned(),
                        num_votes: room.votes.len(),
                        results: room.results.as_ref().map(|tally| VotingResults {
                            tally: tally.clone(),
                            votes: room.votes.values().cloned().collect(),
                        }),
                        num_players: room.clients.len(),
                    }),
                })
                .await
                // Ignore send errors; player could have dropped.
                .ok();
        }
    }

    fn get_new_client_id(&mut self) -> ClientId {
        self.next_client_id += 1;
        ClientId(self.next_client_id - 1)
    }
}

#[derive(serde::Deserialize)]
pub struct NewVoteForm {
    choices: String,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct UserVote {
    name: String,
    selections: Vec<VoteItem>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Command {
    Vote(UserVote),
    Tally,
}

pub async fn start_vote(state: Arc<Mutex<VoteState>>, form: NewVoteForm) -> WebResult<impl Reply> {
    let choices = form
        .choices
        .split('\n')
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
            clients: HashMap::new(),
            results: None,
        },
    );
    let uri = Uri::builder()
        .path_and_query(format!("/vote/{room_id}"))
        .build()
        .unwrap();
    Ok(warp::redirect::see_other(uri))
}

pub async fn handle_vote_client(
    global_state: Arc<Mutex<VoteState>>,
    room_id: String,
    mut ws: WebSocket,
) {
    let (tx, mut rx) = mpsc::channel(1);
    let room_id = RoomId(room_id);
    let client_id;
    {
        let mut gs = global_state.lock().await;
        client_id = gs.get_new_client_id();
        let room = if let Some(room) = gs.rooms.get_mut(&room_id) {
            room
        } else {
            log::debug!("client {client_id:?} gave invalid room {room_id}");
            ws.send(Message::text(
                serde_json::to_string(&ClientNotification {
                    status: ClientStatus::InvalidRoom,
                    vote: None,
                })
                .unwrap(),
            ))
            .await
            .ok();
            return;
        };
        room.clients.insert(client_id, ClientInfo { tx });
        gs.broadcast_state(&room_id).await;
    }
    log::debug!("client {client_id:?} connected to room {room_id}");
    let on_command = |global_state: Arc<Mutex<VoteState>>, room_id, client_id, command| async move {
        log::debug!("Got command: {:?}", command);
        match command {
            Command::Vote(user_vote) => {
                let mut gs = global_state.lock().await;
                let room = gs.rooms.get_mut(&room_id).unwrap();
                if room.results.is_none() {
                    room.votes.insert(client_id, user_vote);
                    gs.broadcast_state(&room_id).await;
                }
            }
            Command::Tally => {
                let mut gs = global_state.lock().await;
                let room = gs.rooms.get_mut(&room_id).unwrap();
                let num_choices = room.choices.len();
                let votes = room
                    .votes
                    .values()
                    .map(|v| &v.selections)
                    .cloned()
                    .collect();
                room.results = Some(ranked_pairs(num_choices, votes));
                log::debug!("Vote results: {:?}", room.results);
                gs.broadcast_state(&room_id).await;
            }
        }
    };
    loop {
        tokio::select! {
            msg = rx.recv() => match msg {
                Some(msg) => {
                    log::debug!("Sending message: {:?}", msg);
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
                    if msg.is_ping() && ws.send(Message::pong("")).await.is_err() {
                        break;
                    }
                    match msg.to_str() {
                        Ok(msg) => match serde_json::from_str::<Command>(msg) {
                            Ok(command) => {
                                on_command(
                                    global_state.clone(),
                                    room_id.clone(),
                                    client_id,
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
                    log::debug!("Client {client_id:?} disconnected.");
                    break
                },
            }
        }
    }
    {
        // cleanup
        let mut gs = global_state.lock().await;
        let room = gs.rooms.get_mut(&room_id).unwrap();
        room.clients.remove(&client_id);
        if room.clients.is_empty() {
            gs.rooms.remove(&room_id);
        } else {
            gs.broadcast_state(&room_id).await;
        }
        log::debug!("client {client_id:?} disconnected.");
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_is_reachable() {
        assert_eq!(is_reachable(0, 1, &[[1].into(), [].into()]), true);
        assert_eq!(is_reachable(1, 0, &[[1].into(), [].into()]), false);
        assert_eq!(
            is_reachable(0, 3, &[[1].into(), [2].into(), [3].into(), [].into()]),
            true
        );
        assert_eq!(
            is_reachable(3, 0, &[[1].into(), [2].into(), [3].into(), [].into()]),
            false
        );
        let complicated_graph = &[
            [4].into(),
            [6].into(),
            [6].into(),
            [4].into(),
            [5].into(),
            [1, 2].into(),
            [0].into(),
        ];
        assert_eq!(is_reachable(0, 1, complicated_graph), true);
        assert_eq!(is_reachable(0, 2, complicated_graph), true);
        assert_eq!(is_reachable(0, 3, complicated_graph), false);
        assert_eq!(is_reachable(0, 4, complicated_graph), true);
        assert_eq!(is_reachable(0, 5, complicated_graph), true);
        assert_eq!(is_reachable(0, 6, complicated_graph), true);
        assert_eq!(is_reachable(2, 0, complicated_graph), true);
        assert_eq!(is_reachable(2, 1, complicated_graph), true);
        assert_eq!(is_reachable(2, 3, complicated_graph), false);
        assert_eq!(is_reachable(2, 4, complicated_graph), true);
        assert_eq!(is_reachable(2, 5, complicated_graph), true);
        assert_eq!(is_reachable(2, 6, complicated_graph), true);
        assert_eq!(is_reachable(3, 0, complicated_graph), true);
        assert_eq!(is_reachable(3, 1, complicated_graph), true);
        assert_eq!(is_reachable(3, 2, complicated_graph), true);
        assert_eq!(is_reachable(3, 4, complicated_graph), true);
        assert_eq!(is_reachable(3, 5, complicated_graph), true);
        assert_eq!(is_reachable(3, 6, complicated_graph), true);
    }

    macro_rules! ballot {
        (@inner $bal:ident $rank:ident $candidate:literal $($tt:tt)*) => {
            $bal.push(VoteItem {
                candidate: $candidate,
                rank: $rank,
            });
            ballot!(@inner $bal $rank $($tt)*)
        };
        (@inner $bal:ident $rank:ident $candidate:literal) => {
            $bal.append(VoteItem {
                candidate: $candidate,
                rank: $rank,
            });
        };
        (@inner $bal:ident $rank:ident > $($tt:tt)*) => {
            $rank += 1;
            ballot!(@inner $bal $rank $($tt)*)
        };
        (@inner $bal:ident $rank:ident = $($tt:tt)*) => {
            ballot!(@inner $bal $rank $($tt)*)
        };
        (@inner $bal:ident $rank:ident) => {};
        (@inner $($tt:tt)*) => {panic!(stringify!($($tt)*))};
        ($($tt:tt)*) => {{
            #[allow(unused_mut)]
            let mut bal = Vec::new();
            #[allow(unused_mut)]
            let mut rank = 1;
            ballot!(@inner bal rank $($tt)*);
            bal
        }}
    }

    macro_rules! ballots {
        ($(($num:literal : $($tt:tt)*))*) => {{
            #[allow(unused_mut)]
            let mut ballots: Vec<Vec<VoteItem>> = Vec::new();
            $(
                for _ in 0..$num {
                    ballots.push(ballot!($($tt)*));
                }
            )*
            ballots
        }}
    }

    #[test]
    fn test_ranked_pairs() {
        assert_eq!(
            ranked_pairs(
                5,
                ballots!(
                    (1: 4 > 1 > 3 > 2 > 0)
                    (1: 1 > 0 > 4 > 3 > 2)
                    (1: 3 > 0 > 4 > 1 > 2)
                    (1: 3 > 4 > 0 > 1 > 2)
                    (1: 2 > 1 > 3 > 0 > 4)
                )
            )
            .ranks[0],
            vec![1, 3, 4],
        );
        assert_eq!(
            ranked_pairs(2, ballots!((1: 0 > 1) (1: 1 > 0))).ranks[0],
            vec![0, 1]
        );

        let ericgorr_example_1 = ballots!(
            (7:0>1>2)
            (5:1>0>2)
            (4:2>0>1)
            (2:1>2>0)
        );
        assert_eq!(ranked_pairs(3, ericgorr_example_1).ranks[0], [0]);

        let ericgorr_example_2 = ballots!(
            (40:0>1>2)
            (35:1>2>0)
            (25:2>0>1)
        );
        assert_eq!(ranked_pairs(3, ericgorr_example_2).ranks[0], [0]);

        let ericgorr_example_3 = ballots!(
            (7:0>1>2)
            (7:1>0>2)
            (2:2>0>1)
            (2:2>1>0)
        );
        assert_eq!(ranked_pairs(3, ericgorr_example_3).ranks[0], [0, 1]);

        let ericgorr_example_4 = ballots!(
            (12:0>3>2>1)
            (3:1>0>2>3)
            (25:1>2>0>3)
            (21:2>1>0>3)
            (12:3>0>1>2)
            (21:3>0>2>1)
            (6:3>1>0>2)
        );
        assert_eq!(ranked_pairs(4, ericgorr_example_4).ranks[0], [1]);

        let ericgorr_interesting_1 = ballots!(
            (12:0>3>2>1)
            (3:1>0>2>3)
            (25:1>2>0>3)
            (21:2>1>0>3)
            (12:3>0>1>2)
            (21:3>0>2>1)
            (6:3>1>0>2)
        );
        assert_eq!(ranked_pairs(4, ericgorr_interesting_1).ranks[0], [1]);

        let ericgorr_interesting_2 = ballots!(
            (280:0>2>3>1)
            (301:1>0>2>3)
            (303:2>1>3>0)
            (356:3>0>1>2)
        );
        assert_eq!(ranked_pairs(4, ericgorr_interesting_2).ranks[0], [0]);
    }
}
