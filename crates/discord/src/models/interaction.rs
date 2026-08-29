use std::collections::HashMap;

use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;

use crate::models::{command::CommandChoice, role::Role, Channel, User};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum InteractionType {
    Ping = 1,
    ApplicationCommand = 2,
    MessageComponent = 3,
    ApplicationCommandAutocomplete = 4,
    ModalSubmit = 5,
}

impl<'de> Deserialize<'de> for InteractionType {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = u8::deserialize(deserializer)?;
        match value {
            1 => Ok(InteractionType::Ping),
            2 => Ok(InteractionType::ApplicationCommand),
            3 => Ok(InteractionType::MessageComponent),
            4 => Ok(InteractionType::ApplicationCommandAutocomplete),
            5 => Ok(InteractionType::ModalSubmit),
            other => Err(de::Error::custom(format!(
                "type d'interaction inconnu: {other}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ResolvedData {
    #[serde(default)]
    pub users: HashMap<String, User>,
    #[serde(default)]
    pub channels: HashMap<String, Channel>,
    #[serde(default)]
    pub roles: HashMap<String, Role>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct InteractionDataOption {
    pub name: String,
    #[serde(default)]
    pub value: Value,
    #[serde(default)]
    pub focused: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct InteractionData {
    pub name: String,
    #[serde(default)]
    pub options: Vec<InteractionDataOption>,
    #[serde(default)]
    pub custom_id: Option<String>,
    #[serde(default)]
    pub resolved: ResolvedData,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Interaction {
    pub id: String,
    pub token: String,
    #[serde(rename = "type")]
    pub kind: InteractionType,
    #[serde(default)]
    pub data: Option<InteractionData>,
    #[serde(default)]
    pub guild_id: Option<String>,
    #[serde(default)]
    pub channel_id: Option<String>,
    #[serde(default)]
    pub member: Option<InteractionMember>,
    #[serde(default)]
    pub user: Option<User>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct InteractionMember {
    pub user: User,
}

impl Interaction {
    pub fn author(&self) -> Option<&User> {
        self.member.as_ref().map(|m| &m.user).or(self.user.as_ref())
    }
}

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum InteractionResponseType {
    Pong = 1,
    ChannelMessageWithSource = 4,
    DeferredChannelMessageWithSource = 5,
    ApplicationCommandAutocompleteResult = 8,
}

impl Serialize for InteractionResponseType {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u8(*self as u8)
    }
}

#[derive(Debug, Serialize)]
pub struct InteractionCallbackData {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub embeds: Vec<super::Embed>,
}

#[derive(Serialize)]
pub struct InteractionResponse {
    #[serde(rename = "type")]
    pub kind: InteractionResponseType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<InteractionCallbackData>,
}

impl InteractionResponse {
    pub fn message(content: impl Into<String>) -> Self {
        Self {
            kind: InteractionResponseType::ChannelMessageWithSource,
            data: Some(InteractionCallbackData {
                content: Some(content.into()),
                embeds: vec![],
            }),
        }
    }

    pub fn embed(embed: super::Embed) -> Self {
        Self {
            kind: InteractionResponseType::ChannelMessageWithSource,
            data: Some(InteractionCallbackData {
                content: None,
                embeds: vec![embed],
            }),
        }
    }
}

#[derive(Serialize)]
pub struct AutocompleteResponse {
    #[serde(rename = "type")]
    kind: InteractionResponseType,
    data: AutocompleteData,
}

#[derive(Serialize)]
struct AutocompleteData {
    choices: Vec<CommandChoice>,
}

impl AutocompleteResponse {
    pub fn new(choices: Vec<CommandChoice>) -> Self {
        Self {
            kind: InteractionResponseType::ApplicationCommandAutocompleteResult,
            data: AutocompleteData { choices },
        }
    }
}
