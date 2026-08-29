use serde::Serialize;

#[derive(Debug, Clone, Default, Serialize)]
pub struct Embed {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub footer: Option<EmbedFooter>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<EmbedImage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumbnail: Option<EmbedImage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<EmbedAuthor>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub fields: Vec<EmbedField>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EmbedField {
    pub name: String,
    pub value: String,
    #[serde(skip_serializing_if = "std::ops::Not::not", default)]
    pub inline: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct EmbedFooter {
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon_url: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EmbedAuthor {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EmbedImage {
    pub url: String,
}

impl Embed {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    pub fn url(mut self, url: impl Into<String>) -> Self {
        self.url = Some(url.into());
        self
    }

    pub fn color(mut self, hex: u32) -> Self {
        self.color = Some(hex);
        self
    }

    pub fn timestamp(mut self, iso8601: impl Into<String>) -> Self {
        self.timestamp = Some(iso8601.into());
        self
    }

    pub fn footer(mut self, text: impl Into<String>) -> Self {
        self.footer = Some(EmbedFooter {
            text: text.into(),
            icon_url: None,
        });
        self
    }

    pub fn footer_with_icon(
        mut self,
        text: impl Into<String>,
        icon_url: impl Into<String>,
    ) -> Self {
        self.footer = Some(EmbedFooter {
            text: text.into(),
            icon_url: Some(icon_url.into()),
        });
        self
    }

    pub fn image(mut self, url: impl Into<String>) -> Self {
        self.image = Some(EmbedImage { url: url.into() });
        self
    }

    pub fn thumbnail(mut self, url: impl Into<String>) -> Self {
        self.thumbnail = Some(EmbedImage { url: url.into() });
        self
    }

    pub fn author(mut self, name: impl Into<String>) -> Self {
        self.author = Some(EmbedAuthor {
            name: name.into(),
            icon_url: None,
            url: None,
        });
        self
    }

    pub fn field(
        mut self,
        name: impl Into<String>,
        value: impl Into<String>,
        inline: bool,
    ) -> Self {
        self.fields.push(EmbedField {
            name: name.into(),
            value: value.into(),
            inline,
        });
        self
    }
}
