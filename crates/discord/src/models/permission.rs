use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct PermissionOverwrite {
    pub id: String,
    #[serde(rename = "type")]
    kind: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    allow: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    deny: Option<String>,
}

pub const PERM_VIEW_CHANNEL: u64 = 1 << 10;
pub const PERM_SEND_MESSAGES: u64 = 1 << 11;

impl PermissionOverwrite {
    pub fn deny_everyone(guild_id: &str, deny: u64) -> Self {
        Self {
            id: guild_id.to_string(),
            kind: 0,
            allow: None,
            deny: Some(deny.to_string()),
        }
    }

    pub fn allow_role(role_id: &str, allow: u64) -> Self {
        Self {
            id: role_id.to_string(),
            kind: 0,
            allow: Some(allow.to_string()),
            deny: None,
        }
    }

    pub fn allow_member(user_id: &str, allow: u64) -> Self {
        Self {
            id: user_id.to_string(),
            kind: 1,
            allow: Some(allow.to_string()),
            deny: None,
        }
    }
}
