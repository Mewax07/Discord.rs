use serde::Deserialize;

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
}
