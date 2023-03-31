//! Types used in decide.pfe.io public API.
#[derive(Debug, Clone, serde::Serialize)]
pub struct CondorcetTally {
    /// totals[a][b] contains the number of votes where candidate a beat b.
    pub totals: Vec<Vec<u64>>,
    // Ranks[0] contains the winner(s), ranks[n] contains the winners if you
    // remove the members of all previous ranks.
    pub ranks: Vec<Vec<usize>>,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct VoteItem {
    pub candidate: usize,
    // Lower is better.
    pub rank: u64,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ClientStatus {
    Connected,
    InvalidRoom,
}

#[derive(Debug, serde::Serialize)]
pub struct VotingResults {
    pub tally: CondorcetTally,
    pub votes: Vec<UserVote>,
}

#[derive(Debug, serde::Serialize)]
pub struct VoteView {
    pub choices: Vec<String>,
    pub your_vote: Option<UserVote>,
    pub num_votes: usize,
    pub num_players: usize,
    pub results: Option<VotingResults>,
}

#[derive(serde::Deserialize)]
pub struct NewVoteForm {
    pub choices: String,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct UserVote {
    pub name: String,
    pub selections: Vec<VoteItem>,
}

/// Data received from a client over websocket.
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Command {
    Vote(UserVote),
    Tally,
}

/// Data serialized and sent to the client in response to a command or other change in state.
#[derive(Debug, serde::Serialize)]
pub struct ClientNotification {
    pub status: ClientStatus,
    pub vote: Option<VoteView>,
}
