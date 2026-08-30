use serde::ser::Serializer;
use serde::Serialize;

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum ButtonStyle {
    Primary = 1,
    Secondary = 2,
    Success = 3,
    Danger = 4,
    Link = 5,
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
pub enum SeparatorSpacing {
    Small = 1,
    Large = 2,
}

impl Serialize for SeparatorSpacing {
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
    Section = 9,
    TextDisplay = 10,
    Thumbnail = 11,
    MediaGallery = 12,
    File = 13,
    Separator = 14,
    Container = 17,
}

impl Serialize for ComponentKind {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_u8(*self as u8)
    }
}

fn is_false(value: &bool) -> bool {
    !*value
}

#[derive(Debug, Clone, Serialize)]
pub struct PartialEmoji {
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UnfurledMedia {
    pub url: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Button {
    #[serde(rename = "type")]
    kind: ComponentKind,
    style: ButtonStyle,
    #[serde(skip_serializing_if = "Option::is_none")]
    label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    custom_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    emoji: Option<PartialEmoji>,
    #[serde(skip_serializing_if = "is_false")]
    disabled: bool,
}

impl Button {
    pub fn new(label: impl Into<String>, custom_id: impl Into<String>, style: ButtonStyle) -> Self {
        Self {
            kind: ComponentKind::Button,
            style,
            label: Some(label.into()),
            custom_id: Some(custom_id.into()),
            url: None,
            emoji: None,
            disabled: false,
        }
    }

    pub fn link(label: impl Into<String>, url: impl Into<String>) -> Self {
        Self {
            kind: ComponentKind::Button,
            style: ButtonStyle::Link,
            label: Some(label.into()),
            custom_id: None,
            url: Some(url.into()),
            emoji: None,
            disabled: false,
        }
    }

    pub fn emoji(mut self, name: impl Into<String>) -> Self {
        self.emoji = Some(PartialEmoji {
            name: Some(name.into()),
            id: None,
        });
        self
    }

    pub fn custom_emoji(mut self, name: impl Into<String>, id: impl Into<String>) -> Self {
        self.emoji = Some(PartialEmoji {
            name: Some(name.into()),
            id: Some(id.into()),
        });
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
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
    #[serde(rename = "default", skip_serializing_if = "is_false")]
    pub preselected: bool,
}

impl SelectOption {
    pub fn new(label: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            value: value.into(),
            description: None,
            emoji: None,
            preselected: false,
        }
    }

    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    pub fn emoji(mut self, name: impl Into<String>) -> Self {
        self.emoji = Some(PartialEmoji {
            name: Some(name.into()),
            id: None,
        });
        self
    }

    pub fn preselected(mut self, preselected: bool) -> Self {
        self.preselected = preselected;
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
    #[serde(skip_serializing_if = "Option::is_none")]
    min_values: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_values: Option<u32>,
    #[serde(skip_serializing_if = "is_false")]
    disabled: bool,
}

impl SelectMenu {
    pub fn new(custom_id: impl Into<String>, options: Vec<SelectOption>) -> Self {
        Self {
            kind: ComponentKind::StringSelect,
            custom_id: custom_id.into(),
            options,
            placeholder: None,
            min_values: None,
            max_values: None,
            disabled: false,
        }
    }

    pub fn placeholder(mut self, text: impl Into<String>) -> Self {
        self.placeholder = Some(text.into());
        self
    }

    pub fn multi(mut self, min: u32, max: u32) -> Self {
        self.min_values = Some(min);
        self.max_values = Some(max);
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
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
    value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    min_length: Option<u32>,
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
            value: None,
            min_length: None,
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

    pub fn value(mut self, value: impl Into<String>) -> Self {
        self.value = Some(value.into());
        self
    }

    pub fn min_length(mut self, len: u32) -> Self {
        self.min_length = Some(len);
        self
    }

    pub fn max_length(mut self, len: u32) -> Self {
        self.max_length = Some(len);
        self
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct TextDisplay {
    #[serde(rename = "type")]
    kind: ComponentKind,
    content: String,
}

impl TextDisplay {
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            kind: ComponentKind::TextDisplay,
            content: content.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Thumbnail {
    #[serde(rename = "type")]
    kind: ComponentKind,
    media: UnfurledMedia,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
}

impl Thumbnail {
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            kind: ComponentKind::Thumbnail,
            media: UnfurledMedia { url: url.into() },
            description: None,
        }
    }

    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct MediaGalleryItem {
    media: UnfurledMedia,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MediaGallery {
    #[serde(rename = "type")]
    kind: ComponentKind,
    items: Vec<MediaGalleryItem>,
}

impl MediaGallery {
    pub fn new(urls: Vec<String>) -> Self {
        Self {
            kind: ComponentKind::MediaGallery,
            items: urls
                .into_iter()
                .map(|url| MediaGalleryItem {
                    media: UnfurledMedia { url },
                    description: None,
                })
                .collect(),
        }
    }

    pub fn single(url: impl Into<String>) -> Self {
        Self::new(vec![url.into()])
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct FileAttachment {
    #[serde(rename = "type")]
    kind: ComponentKind,
    file: UnfurledMedia,
}

impl FileAttachment {
    pub fn new(file_name: &str) -> Self {
        Self {
            kind: ComponentKind::File,
            file: UnfurledMedia {
                url: format!("attachment://{file_name}"),
            },
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Separator {
    #[serde(rename = "type")]
    kind: ComponentKind,
    divider: bool,
    spacing: SeparatorSpacing,
}

impl Separator {
    pub fn line() -> Self {
        Self {
            kind: ComponentKind::Separator,
            divider: true,
            spacing: SeparatorSpacing::Small,
        }
    }

    pub fn gap() -> Self {
        Self {
            kind: ComponentKind::Separator,
            divider: false,
            spacing: SeparatorSpacing::Small,
        }
    }

    pub fn spacing(mut self, spacing: SeparatorSpacing) -> Self {
        self.spacing = spacing;
        self
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Section {
    #[serde(rename = "type")]
    kind: ComponentKind,
    components: Vec<Component>,
    accessory: Box<Component>,
}

impl Section {
    pub fn new(lines: Vec<String>, accessory: impl Into<Component>) -> Self {
        let content = lines
            .into_iter()
            .filter(|line| !line.trim().is_empty())
            .collect::<Vec<_>>()
            .join("\n");

        Self {
            kind: ComponentKind::Section,
            components: vec![Component::TextDisplay(TextDisplay::new(content))],
            accessory: Box::new(accessory.into()),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Container {
    #[serde(rename = "type")]
    kind: ComponentKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    accent_color: Option<u32>,
    components: Vec<Component>,
    #[serde(skip_serializing_if = "is_false")]
    spoiler: bool,
}

impl Container {
    pub fn new(components: Vec<Component>) -> Self {
        Self {
            kind: ComponentKind::Container,
            accent_color: None,
            components,
            spoiler: false,
        }
    }

    pub fn accent(mut self, color: u32) -> Self {
        self.accent_color = Some(color);
        self
    }

    pub fn spoiler(mut self, spoiler: bool) -> Self {
        self.spoiler = spoiler;
        self
    }
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

#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum Component {
    ActionRow(ActionRow),
    Button(Button),
    SelectMenu(SelectMenu),
    TextInput(TextInput),
    Section(Section),
    TextDisplay(TextDisplay),
    Thumbnail(Thumbnail),
    MediaGallery(MediaGallery),
    File(FileAttachment),
    Separator(Separator),
    Container(Container),
}

impl From<ActionRow> for Component {
    fn from(value: ActionRow) -> Self {
        Component::ActionRow(value)
    }
}

impl From<Button> for Component {
    fn from(value: Button) -> Self {
        Component::Button(value)
    }
}

impl From<SelectMenu> for Component {
    fn from(value: SelectMenu) -> Self {
        Component::SelectMenu(value)
    }
}

impl From<TextInput> for Component {
    fn from(value: TextInput) -> Self {
        Component::TextInput(value)
    }
}

impl From<Section> for Component {
    fn from(value: Section) -> Self {
        Component::Section(value)
    }
}

impl From<TextDisplay> for Component {
    fn from(value: TextDisplay) -> Self {
        Component::TextDisplay(value)
    }
}

impl From<Thumbnail> for Component {
    fn from(value: Thumbnail) -> Self {
        Component::Thumbnail(value)
    }
}

impl From<MediaGallery> for Component {
    fn from(value: MediaGallery) -> Self {
        Component::MediaGallery(value)
    }
}

impl From<FileAttachment> for Component {
    fn from(value: FileAttachment) -> Self {
        Component::File(value)
    }
}

impl From<Separator> for Component {
    fn from(value: Separator) -> Self {
        Component::Separator(value)
    }
}

impl From<Container> for Component {
    fn from(value: Container) -> Self {
        Component::Container(value)
    }
}
