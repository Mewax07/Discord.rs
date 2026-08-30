use serde::ser::Serializer;
use serde::Serialize;

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum ButtonStyle {
    Primary = 1,
    Secondary = 2,
    Success = 3,
    Danger = 4,
}

impl Serialize for ButtonStyle {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_u8(*self as u8)
    }
}

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum TextInputStyle {
    Short = 1,
    Paragraph = 2,
}
impl Serialize for TextInputStyle {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_u8(*self as u8)
    }
}

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
enum ComponentKind {
    ActionRow = 1,
    Button = 2,
    StringSelect = 3,
    TextInput = 4,
}

impl Serialize for ComponentKind {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_u8(*self as u8)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct PartialEmoji {
    name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Button {
    #[serde(rename = "type")]
    kind: ComponentKind,
    style: ButtonStyle,
    label: String,
    custom_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    emoji: Option<PartialEmoji>,
}

impl Button {
    pub fn new(label: impl Into<String>, custom_id: impl Into<String>, style: ButtonStyle) -> Self {
        Self {
            kind: ComponentKind::Button,
            style,
            label: label.into(),
            custom_id: custom_id.into(),
            emoji: None,
        }
    }

    pub fn emoji(mut self, name: impl Into<String>) -> Self {
        self.emoji = Some(PartialEmoji { name: name.into() });
        self
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SelectOption {
    pub label: String,
    pub value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emoji: Option<PartialEmoji>,
}

impl SelectOption {
    pub fn new(label: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            value: value.into(),
            description: None,
            emoji: None,
        }
    }

    pub fn description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    pub fn emoji(mut self, name: impl Into<String>) -> Self {
        self.emoji = Some(PartialEmoji { name: name.into() });
        self
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SelectMenu {
    #[serde(rename = "type")]
    kind: ComponentKind,
    custom_id: String,
    options: Vec<SelectOption>,
    #[serde(skip_serializing_if = "Option::is_none")]
    placeholder: Option<String>,
}

impl SelectMenu {
    pub fn new(custom_id: impl Into<String>, options: Vec<SelectOption>) -> Self {
        Self {
            kind: ComponentKind::StringSelect,
            custom_id: custom_id.into(),
            options,
            placeholder: None,
        }
    }

    pub fn placeholder(mut self, text: impl Into<String>) -> Self {
        self.placeholder = Some(text.into());
        self
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct TextInput {
    #[serde(rename = "type")]
    kind: ComponentKind,
    custom_id: String,
    style: TextInputStyle,
    label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    placeholder: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_length: Option<u32>,
    required: bool,
}

impl TextInput {
    pub fn new(
        custom_id: impl Into<String>,
        label: impl Into<String>,
        style: TextInputStyle,
    ) -> Self {
        Self {
            kind: ComponentKind::TextInput,
            custom_id: custom_id.into(),
            style,
            label: label.into(),
            placeholder: None,
            max_length: None,
            required: true,
        }
    }

    pub fn required(mut self, required: bool) -> Self {
        self.required = required;
        self
    }

    pub fn placeholder(mut self, text: impl Into<String>) -> Self {
        self.placeholder = Some(text.into());
        self
    }

    pub fn max_length(mut self, len: u32) -> Self {
        self.max_length = Some(len);
        self
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
enum Component {
    Button(Button),
    SelectMenu(SelectMenu),
    TextInput(TextInput),
}

#[derive(Debug, Clone, Serialize)]
pub struct ActionRow {
    #[serde(rename = "type")]
    kind: ComponentKind,
    components: Vec<Component>,
}

impl ActionRow {
    pub fn buttons(buttons: Vec<Button>) -> Self {
        Self {
            kind: ComponentKind::ActionRow,
            components: buttons.into_iter().map(Component::Button).collect(),
        }
    }

    pub fn select(menu: SelectMenu) -> Self {
        Self {
            kind: ComponentKind::ActionRow,
            components: vec![Component::SelectMenu(menu)],
        }
    }

    pub fn input(text_input: TextInput) -> Self {
        Self {
            kind: ComponentKind::ActionRow,
            components: vec![Component::TextInput(text_input)],
        }
    }
}
