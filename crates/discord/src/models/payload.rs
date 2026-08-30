use serde::Serialize;

use super::{Component, Embed, PollRequest};

pub const SUPPRESS_EMBEDS: u32 = 1 << 2;
pub const EPHEMERAL: u32 = 1 << 6;
pub const SUPPRESS_NOTIFICATIONS: u32 = 1 << 12;
pub const IS_COMPONENTS_V2: u32 = 1 << 15;

#[derive(Debug, Clone, Default, Serialize)]
pub struct AllowedMentions {
    parse: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    users: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    roles: Vec<String>,
}

impl AllowedMentions {
    pub fn none() -> Self {
        Self::default()
    }

    pub fn users(ids: Vec<String>) -> Self {
        Self {
            parse: Vec::new(),
            users: ids,
            roles: Vec::new(),
        }
    }

    pub fn users_and_roles(user_ids: Vec<String>, role_ids: Vec<String>) -> Self {
        Self {
            parse: Vec::new(),
            users: user_ids,
            roles: role_ids,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct MessagePayload {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub embeds: Vec<Embed>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub components: Vec<Component>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flags: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_mentions: Option<AllowedMentions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub poll: Option<PollRequest>,
}

impl MessagePayload {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn text(content: impl Into<String>) -> Self {
        Self {
            content: Some(content.into()),
            ..Self::default()
        }
    }

    pub fn widget(components: Vec<Component>) -> Self {
        Self {
            components,
            flags: Some(IS_COMPONENTS_V2),
            ..Self::default()
        }
    }

    pub fn poll(request: PollRequest) -> Self {
        Self {
            poll: Some(request),
            ..Self::default()
        }
    }

    pub fn embed(embed: Embed) -> Self {
        Self {
            embeds: vec![embed],
            ..Self::default()
        }
    }

    pub fn with_content(mut self, content: impl Into<String>) -> Self {
        self.content = Some(content.into());
        self
    }

    pub fn with_components(mut self, components: Vec<Component>) -> Self {
        self.components = components;
        self
    }

    pub fn with_flag(mut self, flag: u32) -> Self {
        self.flags = Some(self.flags.unwrap_or(0) | flag);
        self
    }

    pub fn ephemeral(self) -> Self {
        self.with_flag(EPHEMERAL)
    }

    pub fn silent(self) -> Self {
        self.with_flag(SUPPRESS_NOTIFICATIONS)
    }

    pub fn mentions(mut self, allowed: AllowedMentions) -> Self {
        self.allowed_mentions = Some(allowed);
        self
    }

    pub fn no_mentions(self) -> Self {
        self.mentions(AllowedMentions::none())
    }

    pub fn is_components_v2(&self) -> bool {
        self.flags.unwrap_or(0) & IS_COMPONENTS_V2 != 0
    }
}
