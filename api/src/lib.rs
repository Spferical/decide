//! Types used in decide.pfe.io public API.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CondorcetTally {
    /// totals[a][b] contains the number of votes where candidate a beat b.
    pub totals: Vec<Vec<u64>>,
    // Ranks[0] contains the winner(s), ranks[n] contains the winners if you
    // remove the members of all previous ranks.
    pub ranks: Vec<Vec<usize>>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct VoteItem {
    pub candidate: usize,
    // Lower is better.
    pub rank: u64,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EndRule {
    #[default]
    Anyone,
    Creator,
    Password,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClientStatus {
    Connected,
    InvalidRoom,
    InvalidUuid,
    InvalidPassword,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VotingResults {
    pub tally: CondorcetTally,
    pub votes: Vec<UserVote>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VoteView {
    pub choices: Vec<String>,
    pub your_vote: Option<UserVote>,
    pub num_votes: usize,
    pub num_players: usize,
    pub tallied: bool,
    pub live_results: bool,
    pub results: Option<VotingResults>,
    pub end_rule: EndRule,
    pub is_creator: bool,
}

#[derive(Clone, Debug, Deserialize)]
pub struct NewVoteForm {
    pub choices: String,
    #[serde(rename = "live-results")]
    #[serde(default)]
    pub live_results: bool,
    #[serde(rename = "creator-id")]
    pub creator_id: String,
    #[serde(rename = "end-rule")]
    pub end_rule: EndRule,
    #[serde(rename = "end-password")]
    pub end_password: Option<String>,
}

#[derive(Deserialize)]
pub struct VoteWebsocketQueryParams {
    /// Unique per client per room. Used to allow reconnections.
    pub id: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct UserVote {
    pub name: String,
    pub selections: Vec<VoteItem>,
}

/// Data received from a client over websocket.
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Command {
    Vote(UserVote),
    Tally(Option<String>),
}

impl Command {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Vote(_) => "vote",
            Self::Tally(_) => "tally",
        }
    }
}

/// Data serialized and sent to the client in response to a command or other change in state.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ClientNotification {
    pub status: ClientStatus,
    pub vote: Option<VoteView>,
}
