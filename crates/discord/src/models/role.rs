use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Role {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub color: u32,
    #[serde(default)]
    pub position: i32,
    #[serde(default)]
    pub permissions: String,
    #[serde(default)]
    pub managed: bool,
}

impl Role {
    pub fn permission_bits(&self) -> u64 {
        self.permissions.parse::<u64>().unwrap_or(0)
    }
}

impl Role {
    pub fn mention(&self) -> String {
        format!("<@&{}>", self.id)
    }
}
