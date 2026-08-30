use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct PermissionOverwrite {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deny: Option<String>,
}

pub const PERM_ADMINISTRATOR: u64 = 1 << 3;
pub const PERM_MANAGE_CHANNELS: u64 = 1 << 4;
pub const PERM_MANAGE_GUILD: u64 = 1 << 5;
pub const PERM_ADD_REACTIONS: u64 = 1 << 6;
pub const PERM_VIEW_CHANNEL: u64 = 1 << 10;
pub const PERM_SEND_MESSAGES: u64 = 1 << 11;
pub const PERM_MANAGE_MESSAGES: u64 = 1 << 13;
pub const PERM_EMBED_LINKS: u64 = 1 << 14;
pub const PERM_ATTACH_FILES: u64 = 1 << 15;
pub const PERM_READ_MESSAGE_HISTORY: u64 = 1 << 16;
pub const PERM_MENTION_EVERYONE: u64 = 1 << 17;
pub const PERM_MANAGE_ROLES: u64 = 1 << 28;

pub const OVERWRITE_ROLE: u8 = 0;
pub const OVERWRITE_MEMBER: u8 = 1;

pub const TICKET_MEMBER_PERMS: u64 =
    PERM_VIEW_CHANNEL | PERM_SEND_MESSAGES | PERM_ATTACH_FILES | PERM_READ_MESSAGE_HISTORY;

impl PermissionOverwrite {
    pub fn new(id: &str, kind: u8, allow: u64, deny: u64) -> Self {
        Self {
            id: id.to_string(),
            kind,
            allow: (allow != 0).then(|| allow.to_string()),
            deny: (deny != 0).then(|| deny.to_string()),
        }
    }

    pub fn deny_everyone(guild_id: &str, deny: u64) -> Self {
        Self::new(guild_id, OVERWRITE_ROLE, 0, deny)
    }

    pub fn allow_role(role_id: &str, allow: u64) -> Self {
        Self::new(role_id, OVERWRITE_ROLE, allow, 0)
    }

    pub fn allow_member(user_id: &str, allow: u64) -> Self {
        Self::new(user_id, OVERWRITE_MEMBER, allow, 0)
    }
}
