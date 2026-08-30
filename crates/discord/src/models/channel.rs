use serde::{Deserialize, Serialize};

use super::PermissionOverwrite;

#[derive(Debug, Clone, Deserialize)]
pub struct Channel {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: u8,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub guild_id: Option<String>,
    #[serde(default)]
    pub parent_id: Option<String>,
    #[serde(default)]
    pub topic: Option<String>,
}

pub const CHANNEL_TYPE_GUILD_TEXT: u8 = 0;
pub const CHANNEL_TYPE_GUILD_VOICE: u8 = 2;
pub const CHANNEL_TYPE_GUILD_CATEGORY: u8 = 4;

impl Channel {
    pub fn mention(&self) -> String {
        format!("<#{}>", self.id)
    }

    pub fn is_category(&self) -> bool {
        self.kind == CHANNEL_TYPE_GUILD_CATEGORY
    }

    pub fn is_text(&self) -> bool {
        self.kind == CHANNEL_TYPE_GUILD_TEXT
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct NewChannel {
    pub name: String,
    #[serde(rename = "type")]
    pub kind: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub topic: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub permission_overwrites: Vec<PermissionOverwrite>,
}

impl NewChannel {
    pub fn text(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            kind: CHANNEL_TYPE_GUILD_TEXT,
            parent_id: None,
            topic: None,
            permission_overwrites: Vec::new(),
        }
    }

    pub fn parent(mut self, parent_id: Option<&str>) -> Self {
        self.parent_id = parent_id.map(String::from);
        self
    }

    pub fn topic(mut self, topic: impl Into<String>) -> Self {
        self.topic = Some(topic.into());
        self
    }

    pub fn overwrites(mut self, overwrites: Vec<PermissionOverwrite>) -> Self {
        self.permission_overwrites = overwrites;
        self
    }
}
