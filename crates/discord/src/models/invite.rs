use serde::Deserialize;

use super::User;

#[derive(Debug, Clone, Deserialize)]
pub struct Invite {
    pub code: String,
    #[serde(default)]
    pub uses: u32,
    #[serde(default)]
    pub inviter: Option<User>,
}
