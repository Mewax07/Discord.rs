use serde::de::{self, Deserializer};
use serde::ser::Serializer;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{ActionRow, Channel, CommandChoice, Role, User};

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
                "unknown interaction type: {other}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ResolvedData {
    #[serde(default)]
    pub users: std::collections::HashMap<String, User>,
    #[serde(default)]
    pub channels: std::collections::HashMap<String, Channel>,
    #[serde(default)]
    pub roles: std::collections::HashMap<String, Role>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct InteractionDataOption {
    pub name: String,
    #[serde(default)]
    pub value: Value,
    #[serde(default)]
    pub focused: bool,
    #[serde(default)]
    pub options: Vec<InteractionDataOption>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ModalFieldValue {
    pub custom_id: String,
    #[serde(default)]
    pub value: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ModalActionRowData {
    #[serde(default)]
    pub components: Vec<ModalFieldValue>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct InteractionData {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub options: Vec<InteractionDataOption>,
    #[serde(default)]
    pub custom_id: Option<String>,
    #[serde(default)]
    pub resolved: ResolvedData,
    #[serde(default)]
    pub values: Vec<String>,
    #[serde(default)]
    pub components: Vec<ModalActionRowData>,
}

impl InteractionData {
    pub fn modal_value(&self, custom_id: &str) -> Option<&str> {
        self.components
            .iter()
            .flat_map(|row| row.components.iter())
            .find(|field| field.custom_id == custom_id)
            .map(|field| field.value.as_str())
    }
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
    Modal = 9,
}

impl Serialize for InteractionResponseType {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u8(*self as u8)
    }
}

pub const EPHEMERAL: u32 = 1 << 6;

#[derive(Debug, Serialize)]
pub struct InteractionCallbackData {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub embeds: Vec<super::Embed>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub components: Vec<ActionRow>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flags: Option<u32>,
}

#[derive(Serialize)]
pub struct InteractionResponse {
    #[serde(rename = "type")]
    kind: InteractionResponseType,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<InteractionCallbackData>,
}

impl InteractionResponse {
    pub fn message(content: impl Into<String>) -> Self {
        Self {
            kind: InteractionResponseType::ChannelMessageWithSource,
            data: Some(InteractionCallbackData {
                content: Some(content.into()),
                embeds: vec![],
                components: vec![],
                flags: None,
            }),
        }
    }

    pub fn embed(embed: super::Embed) -> Self {
        Self {
            kind: InteractionResponseType::ChannelMessageWithSource,
            data: Some(InteractionCallbackData {
                content: None,
                embeds: vec![embed],
                components: vec![],
                flags: None,
            }),
        }
    }

    pub fn with_components(mut self, components: Vec<ActionRow>) -> Self {
        if let Some(d) = &mut self.data {
            d.components = components;
        }
        self
    }

    pub fn ephemeral(mut self) -> Self {
        if let Some(d) = &mut self.data {
            d.flags = Some(EPHEMERAL);
        }
        self
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

#[derive(Serialize)]
pub struct ModalResponse {
    #[serde(rename = "type")]
    kind: InteractionResponseType,
    data: ModalCallbackData,
}

#[derive(Serialize)]
struct ModalCallbackData {
    custom_id: String,
    title: String,
    components: Vec<ActionRow>,
}

impl ModalResponse {
    pub fn new(
        custom_id: impl Into<String>,
        title: impl Into<String>,
        components: Vec<ActionRow>,
    ) -> Self {
        Self {
            kind: InteractionResponseType::Modal,
            data: ModalCallbackData {
                custom_id: custom_id.into(),
                title: title.into(),
                components,
            },
        }
    }
}
