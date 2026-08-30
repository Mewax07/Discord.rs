use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::JsonStore;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PollRecord {
    pub guild_id: String,
    pub channel_id: String,
    pub message_id: String,
    pub question: String,
    pub options: Vec<String>,
    #[serde(default)]
    pub author_id: String,
    #[serde(default)]
    pub multi: bool,
    #[serde(default)]
    pub created_at: u64,
    pub ends_at: u64,
    pub ended: bool,
}

impl PollRecord {
    pub fn link(&self) -> String {
        format!(
            "https://discord.com/channels/{}/{}/{}",
            self.guild_id, self.channel_id, self.message_id
        )
    }
}

#[derive(Default, Serialize, Deserialize)]
struct PollData {
    #[serde(default)]
    polls: HashMap<String, PollRecord>,
}

pub struct PollStore {
    store: JsonStore<PollData>,
}

impl PollStore {
    pub fn open(path: impl Into<PathBuf>) -> Self {
        Self {
            store: JsonStore::open(path),
        }
    }

    pub fn insert(&self, message_id: &str, record: PollRecord) {
        self.store.write(|d| {
            d.polls.insert(message_id.to_string(), record);
        });
    }

    pub fn get(&self, message_id: &str) -> Option<PollRecord> {
        self.store.read(|d| d.polls.get(message_id).cloned())
    }

    pub fn update(&self, message_id: &str, f: impl FnOnce(&mut PollRecord)) -> Option<PollRecord> {
        self.store.write(|d| {
            let record = d.polls.get_mut(message_id)?;
            f(record);
            Some(record.clone())
        })
    }

    pub fn all_pending(&self) -> Vec<(String, PollRecord)> {
        self.store.read(|d| {
            d.polls
                .iter()
                .filter(|(_, r)| !r.ended)
                .map(|(id, r)| (id.clone(), r.clone()))
                .collect()
        })
    }
}
