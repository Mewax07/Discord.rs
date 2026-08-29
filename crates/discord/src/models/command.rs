use serde::{Deserialize, Serialize, Serializer};

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
}

#[derive(Debug, Clone, Serialize)]
pub struct CommandDefinition {
    pub name: String,
    pub description: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<CommandOption>,
}

impl CommandDefinition {
    pub fn new(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            options: Vec::new(),
        }
    }

    pub fn option(mut self, option: CommandOption) -> Self {
        self.options.push(option);
        self
    }
}

#[derive(Debug, Deserialize)]
pub struct RegisteredCommand {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct ApplicationInfo {
    pub id: String,
}
