use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Role {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub color: u32,
    #[serde(default)]
    pub position: i32,
}
