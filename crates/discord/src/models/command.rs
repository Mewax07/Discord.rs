use serde::ser::Serializer;
use serde::Serialize;

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum CommandOptionType {
    SubCommand = 1,
    SubCommandGroup = 2,
    String = 3,
    Integer = 4,
    Boolean = 5,
    User = 6,
    Channel = 7,
    Role = 8,
    Mentionable = 9,
    Number = 10,
    Attachment = 11,
}

impl Serialize for CommandOptionType {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u8(*self as u8)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct CommandChoice {
    pub name: String,
    pub value: serde_json::Value,
}

fn is_false(b: &bool) -> bool {
    !*b
}

#[derive(Debug, Clone, Serialize)]
pub struct CommandOption {
    #[serde(rename = "type")]
    pub kind: CommandOptionType,
    pub name: String,
    pub description: String,
    #[serde(default, skip_serializing_if = "is_false")]
    pub required: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub choices: Vec<CommandChoice>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub autocomplete: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<CommandOption>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub channel_types: Vec<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_value: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_value: Option<i64>,
}

impl CommandOption {
    fn new(
        kind: CommandOptionType,
        name: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            name: name.into(),
            description: description.into(),
            required: false,
            choices: Vec::new(),
            autocomplete: false,
            options: Vec::new(),
            channel_types: Vec::new(),
            min_value: None,
            max_value: None,
        }
    }

    pub fn string(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self::new(CommandOptionType::String, name, description)
    }

    pub fn integer(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self::new(CommandOptionType::Integer, name, description)
    }

    pub fn boolean(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self::new(CommandOptionType::Boolean, name, description)
    }

    pub fn user(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self::new(CommandOptionType::User, name, description)
    }

    pub fn channel(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self::new(CommandOptionType::Channel, name, description)
    }

    pub fn role(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self::new(CommandOptionType::Role, name, description)
    }

    pub fn number(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self::new(CommandOptionType::Number, name, description)
    }

    pub fn subcommand(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self::new(CommandOptionType::SubCommand, name, description)
    }

    pub fn group(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self::new(CommandOptionType::SubCommandGroup, name, description)
    }

    pub fn attachment(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self::new(CommandOptionType::Attachment, name, description)
    }

    pub fn min_value(mut self, value: i64) -> Self {
        self.min_value = Some(value);
        self
    }

    pub fn max_value(mut self, value: i64) -> Self {
        self.max_value = Some(value);
        self
    }

    pub fn required(mut self, required: bool) -> Self {
        self.required = required;
        self
    }

    pub fn choice(mut self, name: impl Into<String>, value: impl Into<serde_json::Value>) -> Self {
        self.choices.push(CommandChoice {
            name: name.into(),
            value: value.into(),
        });
        self
    }

    pub fn autocomplete(mut self, enabled: bool) -> Self {
        self.autocomplete = enabled;
        self
    }

    pub fn option(mut self, opt: CommandOption) -> Self {
        self.options.push(opt);
        self
    }

    pub fn channel_types(mut self, types: Vec<u8>) -> Self {
        self.channel_types = types;
        self
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct CommandDefinition {
    pub name: String,
    pub description: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<CommandOption>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_member_permissions: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dm_permission: Option<bool>,
}

impl CommandDefinition {
    pub fn new(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            options: Vec::new(),
            default_member_permissions: None,
            dm_permission: None,
        }
    }

    pub fn option(mut self, option: CommandOption) -> Self {
        self.options.push(option);
        self
    }

    pub fn required_permissions(mut self, bits: u64) -> Self {
        self.default_member_permissions = Some(bits.to_string());
        self
    }

    pub fn guild_only(mut self) -> Self {
        self.dm_permission = Some(false);
        self
    }
}

#[derive(Debug, serde::Deserialize)]
pub struct RegisteredCommand {
    pub id: String,
    pub name: String,
}

#[derive(Debug, serde::Deserialize)]
pub struct ApplicationInfo {
    pub id: String,
}
