use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::JsonStore;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GiveawayRecord {
    pub guild_id: String,
    pub channel_id: String,
    pub message_id: String,
    pub host_id: String,
    pub prize: String,
    pub winner_count: u32,
    pub ends_at: u64,
    #[serde(default)]
    pub entrants: Vec<String>,
    pub ended: bool,
    #[serde(default)]
    pub created_at: u64,
    #[serde(default)]
    pub required_role_id: Option<String>,
    #[serde(default)]
    pub winners: Vec<String>,
    #[serde(default = "text_reward")]
    pub reward_kind: String,
    #[serde(default)]
    pub reward_plan: Option<String>,
    #[serde(default)]
    pub reward_days: Option<u64>,
    #[serde(default)]
    pub issued: Vec<(String, String)>,
}

fn text_reward() -> String {
    "text".to_string()
}

impl GiveawayRecord {
    pub fn link(&self) -> String {
        format!(
            "https://discord.com/channels/{}/{}/{}",
            self.guild_id, self.channel_id, self.message_id
        )
    }

    pub fn has_entered(&self, user_id: &str) -> bool {
        self.entrants.iter().any(|id| id == user_id)
    }

    pub fn toggle(&mut self, user_id: &str) -> bool {
        match self.entrants.iter().position(|id| id == user_id) {
            Some(index) => {
                self.entrants.remove(index);
                false
            }
            None => {
                self.entrants.push(user_id.to_string());
                true
            }
        }
    }
}

#[derive(Default, Serialize, Deserialize)]
struct GiveawayData {
    #[serde(default)]
    giveaways: HashMap<String, GiveawayRecord>,
}

pub struct GiveawayStore {
    store: JsonStore<GiveawayData>,
}

impl GiveawayStore {
    pub fn open(path: impl Into<PathBuf>) -> Self {
        Self {
            store: JsonStore::open(path),
        }
    }

    pub fn insert(&self, message_id: &str, record: GiveawayRecord) {
        self.store.write(|d| {
            d.giveaways.insert(message_id.to_string(), record);
        });
    }

    pub fn get(&self, message_id: &str) -> Option<GiveawayRecord> {
        self.store.read(|d| d.giveaways.get(message_id).cloned())
    }

    pub fn update(
        &self,
        message_id: &str,
        f: impl FnOnce(&mut GiveawayRecord),
    ) -> Option<GiveawayRecord> {
        self.store.write(|d| {
            let record = d.giveaways.get_mut(message_id)?;
            f(record);
            Some(record.clone())
        })
    }

    pub fn all_pending(&self) -> Vec<(String, GiveawayRecord)> {
        self.store.read(|d| {
            d.giveaways
                .iter()
                .filter(|(_, r)| !r.ended)
                .map(|(id, r)| (id.clone(), r.clone()))
                .collect()
        })
    }
}
