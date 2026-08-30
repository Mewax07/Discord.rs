use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::JsonStore;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GuildConfig {
    pub ticket_category_id: Option<String>,
    pub staff_role_id: Option<String>,
    #[serde(default)]
    pub category_roles: HashMap<String, String>,
    #[serde(default)]
    pub ticket_counter: u64,
}

#[derive(Default, Serialize, Deserialize)]
struct ConfigData {
    #[serde(default)]
    guilds: HashMap<String, GuildConfig>,
}

pub struct ConfigStore {
    store: JsonStore<ConfigData>,
}

impl ConfigStore {
    pub fn open(path: impl Into<PathBuf>) -> Self {
        Self {
            store: JsonStore::open(path),
        }
    }

    pub fn get(&self, guild_id: &str) -> GuildConfig {
        self.store
            .read(|d| d.guilds.get(guild_id).cloned().unwrap_or_default())
    }

    pub fn update(&self, guild_id: &str, f: impl FnOnce(&mut GuildConfig)) {
        self.store
            .write(|d| f(d.guilds.entry(guild_id.to_string()).or_default()));
    }

    pub fn next_ticket_number(&self, guild_id: &str) -> u64 {
        self.store.write(|d| {
            let cfg = d.guilds.entry(guild_id.to_string()).or_default();
            cfg.ticket_counter += 1;
            cfg.ticket_counter
        })
    }
}
