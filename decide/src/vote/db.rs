use std::{collections::HashMap, str::FromStr};

use serde::{Deserialize, Serialize};
use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
    Row,
};

use super::util::{ClientId, RoomId};

#[derive(Clone, Serialize, Deserialize)]
pub struct RoomV1 {
    pub choices: Vec<String>,
    pub votes: HashMap<ClientId, decide_api::UserVote>,
    pub tallied: bool,
    #[serde(default)]
    pub live_results: bool,
}

#[derive(Clone, Serialize, Deserialize)]
pub enum Room {
    V1(RoomV1),
}

pub struct RoomOptions {
    pub choices: Vec<String>,
    pub live_results: bool,
}

/*
CREATE TABLE room (
  id TEXT PRIMARY KEY,
  state JSON,
  last_active DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
);
*/

pub(crate) struct Db {
    db_pool: sqlx::Pool<sqlx::Sqlite>,
}

impl Db {
    pub async fn init(db_url: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let connection_options = SqliteConnectOptions::from_str(db_url)?
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal);
        let db_pool = SqlitePoolOptions::new()
            .connect_with(connection_options)
            .await?;
        sqlx::migrate!("./migrations").run(&db_pool).await?;
        Ok(Self { db_pool })
    }

    pub async fn create_room(&self, opts: RoomOptions) -> RoomId {
        let room_id = RoomId::new_random();
        let room_state = Room::V1(RoomV1 {
            choices: opts.choices,
            votes: HashMap::new(),
            tallied: false,
            live_results: opts.live_results,
        });
        let room_state_json =
            serde_json::to_string(&room_state).expect("Failed to serialize initial room state");
        sqlx::query("INSERT INTO room (id, state) VALUES (?, ?)")
            .bind(&room_id.0)
            .bind(room_state_json)
            .execute(&self.db_pool)
            .await
            .expect("Failed to insert new room");
        room_id
    }

    pub async fn read_room_state(&self, room_id: &RoomId) -> Option<Room> {
        let row = sqlx::query("SELECT state FROM room WHERE id = ?")
            .bind(&room_id.0)
            .fetch_one(&self.db_pool)
            .await
            .ok()?;
        let room_state_json: String = row.get(0);
        match serde_json::from_str::<Room>(&room_state_json) {
            Ok(room_state) => Some(room_state),
            Err(e) => {
                log::error!("Failed to deserialize room {room_id} state: {e}");
                None
            }
        }
    }

    pub async fn write_room_state(&self, room_id: &RoomId, room_state: Room) -> bool {
        let room_state_json =
            serde_json::to_string(&room_state).expect("Failed to serialize room state");
        // Note: silently fails if room does not exist in database.
        // Returns false if the room does not exist.
        sqlx::query("UPDATE room SET state = ?, last_active = CURRENT_TIMESTAMP WHERE id = ?")
            .bind(room_state_json)
            .bind(&room_id.0)
            .execute(&self.db_pool)
            .await
            .expect("Failed to update room")
            .rows_affected()
            == 1
    }

    pub async fn bump_room_activity(&self, room_id: &RoomId) {
        sqlx::query("UPDATE room SET last_active = CURRENT_TIMESTAMP WHERE id = ?")
            .bind(&room_id.0)
            .execute(&self.db_pool)
            .await
            .expect("Failed to update room last active");
    }
}
