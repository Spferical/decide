use std::str::FromStr;

use rand::distributions::DistString;
use uuid::Uuid;
use serde::{Serialize, Deserialize};

/// Each websocket connection is a unique player.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ClientId(pub Uuid);

impl Serialize for ClientId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.0.to_string())
    }
}

impl<'de> Deserialize<'de> for ClientId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(Self(Uuid::from_str(&s).map_err(serde::de::Error::custom)?))
    }
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct RoomId(pub String);

impl RoomId {
    pub fn new_random() -> Self {
        Self(rand::distributions::Alphanumeric.sample_string(&mut rand::thread_rng(), 8))
    }
}

impl std::fmt::Display for RoomId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}