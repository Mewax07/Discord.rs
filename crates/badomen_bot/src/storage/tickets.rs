use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::JsonStore;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TicketRecord {
    pub guild_id: String,
    #[serde(default)]
    pub channel_id: String,
    pub opener_id: String,
    pub category: String,
    pub opened_at: u64,
    #[serde(default)]
    pub claimed_by: Option<String>,
    #[serde(default)]
    pub on_hold: bool,
    #[serde(default)]
    pub number: u64,
    #[serde(default)]
    pub subject: Option<String>,
    #[serde(default)]
    pub context: Vec<(String, String)>,
    #[serde(default)]
    pub invited: Vec<String>,
}

impl TicketRecord {
    pub fn status_label(&self) -> &'static str {
        if self.on_hold {
            "On hold"
        } else if self.claimed_by.is_some() {
            "In progress"
        } else {
            "Waiting for staff"
        }
    }
}

#[derive(Default, Serialize, Deserialize)]
struct TicketData {
    #[serde(default)]
    tickets: HashMap<String, TicketRecord>,
}

pub struct TicketStore {
    store: JsonStore<TicketData>,
}

impl TicketStore {
    pub fn open(path: impl Into<PathBuf>) -> Self {
        Self {
            store: JsonStore::open(path),
        }
    }

    pub fn insert(&self, channel_id: &str, record: TicketRecord) {
        self.store.write(|d| {
            d.tickets.insert(channel_id.to_string(), record);
        });
    }

    pub fn get(&self, channel_id: &str) -> Option<TicketRecord> {
        self.store.read(|d| d.tickets.get(channel_id).cloned())
    }

    pub fn update(
        &self,
        channel_id: &str,
        f: impl FnOnce(&mut TicketRecord),
    ) -> Option<TicketRecord> {
        self.store.write(|d| {
            let record = d.tickets.get_mut(channel_id)?;
            f(record);
            Some(record.clone())
        })
    }

    pub fn remove(&self, channel_id: &str) -> Option<TicketRecord> {
        self.store.write(|d| d.tickets.remove(channel_id))
    }

    pub fn open_for(&self, guild_id: &str, user_id: &str) -> Vec<TicketRecord> {
        self.store.read(|d| {
            d.tickets
                .values()
                .filter(|r| r.guild_id == guild_id && r.opener_id == user_id)
                .cloned()
                .collect()
        })
    }
}
