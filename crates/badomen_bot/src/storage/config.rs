use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::JsonStore;

pub const LOG_TICKETS: &str = "tickets";
pub const LOG_MEMBERS: &str = "members";
pub const LOG_POLLS: &str = "polls";
pub const LOG_GIVEAWAYS: &str = "giveaways";
pub const LOG_CONFIG: &str = "config";
pub const LOG_SYSTEM: &str = "system";
pub const LOG_DEFAULT: &str = "default";

pub const LOG_KEYS: &[(&str, &str)] = &[
    (LOG_DEFAULT, "Fallback for every category"),
    (LOG_TICKETS, "Ticket openings, claims and closures"),
    (LOG_MEMBERS, "Rules acceptance and role changes"),
    (LOG_POLLS, "Poll creation and results"),
    (LOG_GIVEAWAYS, "Giveaway creation, winners and rerolls"),
    (LOG_CONFIG, "Configuration changes"),
    (LOG_SYSTEM, "Startup, shutdown and internal errors"),
];

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Brand {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub accent: Option<u32>,
    #[serde(default)]
    pub logo_url: Option<String>,
    #[serde(default)]
    pub banner_url: Option<String>,
    #[serde(default)]
    pub footer: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleEntry {
    pub title: String,
    pub body: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuildConfig {
    #[serde(default)]
    pub ticket_category_id: Option<String>,
    #[serde(default)]
    pub staff_role_id: Option<String>,
    #[serde(default)]
    pub category_roles: HashMap<String, String>,
    #[serde(default)]
    pub ticket_counter: u64,
    #[serde(default)]
    pub ticket_log_channel_id: Option<String>,
    #[serde(default = "yes")]
    pub ticket_transcripts: bool,
    #[serde(default = "yes")]
    pub ticket_dm_summary: bool,
    #[serde(default)]
    pub member_role_id: Option<String>,
    #[serde(default)]
    pub self_roles: HashMap<String, String>,
    #[serde(default)]
    pub log_channels: HashMap<String, String>,
    #[serde(default)]
    pub rules: Vec<RuleEntry>,
    #[serde(default)]
    pub rules_channel_id: Option<String>,
    #[serde(default)]
    pub rules_message_id: Option<String>,
    #[serde(default)]
    pub rules_updated_at: u64,
    #[serde(default)]
    pub brand: Brand,
}

fn yes() -> bool {
    true
}

impl Default for GuildConfig {
    fn default() -> Self {
        Self {
            ticket_category_id: None,
            staff_role_id: None,
            category_roles: HashMap::new(),
            ticket_counter: 0,
            ticket_log_channel_id: None,
            ticket_transcripts: true,
            ticket_dm_summary: true,
            member_role_id: None,
            self_roles: HashMap::new(),
            log_channels: HashMap::new(),
            rules: Vec::new(),
            rules_channel_id: None,
            rules_message_id: None,
            rules_updated_at: 0,
            brand: Brand::default(),
        }
    }
}

impl GuildConfig {
    pub fn log_channel(&self, key: &str) -> Option<&str> {
        if let Some(id) = self.log_channels.get(key) {
            return Some(id.as_str());
        }
        if key == LOG_TICKETS {
            if let Some(id) = &self.ticket_log_channel_id {
                return Some(id.as_str());
            }
        }
        self.log_channels.get(LOG_DEFAULT).map(String::as_str)
    }

    pub fn is_staff(&self, roles: &[String]) -> bool {
        if let Some(staff) = &self.staff_role_id {
            if roles.iter().any(|r| r == staff) {
                return true;
            }
        }
        self.category_roles.values().any(|r| roles.contains(r))
    }
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

    pub fn brand(&self, guild_id: &str) -> Brand {
        self.store.read(|d| {
            d.guilds
                .get(guild_id)
                .map(|c| c.brand.clone())
                .unwrap_or_default()
        })
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
