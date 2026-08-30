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

impl Channel {
    pub fn mention(&self) -> String {
        format!("<#{}>", self.id)
    }
}
