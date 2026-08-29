use serde::Deserialize;

use super::User;

#[derive(Debug, Clone, Deserialize)]
pub struct Message {
    pub id: String,
    pub channel_id: String,
    #[serde(default)]
    pub guild_id: Option<String>,
    pub content: String,
    pub author: User,
}
