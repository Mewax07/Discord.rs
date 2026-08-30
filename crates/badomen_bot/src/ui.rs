use discord::models::{
    ActionRow, Button, Component, Container, FileAttachment, MediaGallery, Section, SelectMenu,
    Separator, TextDisplay, Thumbnail,
};

use crate::storage::Brand;

pub const ACCENT: u32 = 0x8B5CF6;
pub const SUCCESS: u32 = 0x3BA55D;
pub const WARNING: u32 = 0xE0A23C;
pub const DANGER: u32 = 0xD83C3E;
pub const NEUTRAL: u32 = 0x4E5058;

pub struct Theme {
    pub accent: u32,
    pub name: String,
    pub footer: String,
    pub logo: Option<String>,
    pub banner: Option<String>,
}

impl Theme {
    pub fn from_brand(brand: &Brand) -> Self {
        let name = brand.name.clone().unwrap_or_else(|| "BadOmen".to_string());
        let footer = brand
            .footer
            .clone()
            .unwrap_or_else(|| format!("{name} · Community services"));
        Self {
            accent: brand.accent.unwrap_or(ACCENT),
            name,
            footer,
            logo: brand.logo_url.clone(),
            banner: brand.banner_url.clone(),
        }
    }
}

pub fn title(text: impl AsRef<str>) -> String {
    format!("## {}", text.as_ref())
}

pub fn subtitle(text: impl AsRef<str>) -> String {
    format!("### {}", text.as_ref())
}

pub fn note(text: impl AsRef<str>) -> String {
    format!("-# {}", text.as_ref())
}

pub fn kv(name: impl AsRef<str>, value: impl AsRef<str>) -> String {
    format!("**{}** · {}", name.as_ref(), value.as_ref())
}

pub fn entry(name: impl AsRef<str>, value: impl AsRef<str>) -> String {
    format!("**{}**\n{}", name.as_ref(), value.as_ref())
}

pub fn code(text: impl AsRef<str>) -> String {
    format!("`{}`", text.as_ref())
}

pub fn italic(text: impl AsRef<str>) -> String {
    format!("*{}*", text.as_ref())
}

pub fn bullets(items: &[String]) -> String {
    items
        .iter()
        .map(|item| format!("- {item}"))
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn unset() -> String {
    "*not configured*".to_string()
}

pub fn relative(unix: u64) -> String {
    format!("<t:{unix}:R>")
}

pub fn full_date(unix: u64) -> String {
    format!("<t:{unix}:f>")
}

pub fn short_date(unix: u64) -> String {
    format!("<t:{unix}:d>")
}

pub fn user(id: &str) -> String {
    format!("<@{id}>")
}

pub fn role(id: &str) -> String {
    format!("<@&{id}>")
}

pub fn channel(id: &str) -> String {
    format!("<#{id}>")
}

pub fn progress(value: usize, total: usize, width: usize) -> String {
    let filled = if total == 0 {
        0
    } else {
        (value * width + total / 2) / total
    };
    let filled = filled.min(width);
    format!(
        "`{}{}`",
        "\u{2588}".repeat(filled),
        "\u{2591}".repeat(width - filled)
    )
}

pub fn percent(value: usize, total: usize) -> usize {
    if total == 0 {
        0
    } else {
        (value * 100 + total / 2) / total
    }
}

pub fn text(content: impl Into<String>) -> Component {
    TextDisplay::new(content).into()
}

pub fn lines(parts: Vec<String>) -> Component {
    text(
        parts
            .into_iter()
            .filter(|part| !part.trim().is_empty())
            .collect::<Vec<_>>()
            .join("\n"),
    )
}

pub fn divider() -> Component {
    Separator::line().into()
}

pub fn gap() -> Component {
    Separator::gap().into()
}

pub fn row(buttons: Vec<Button>) -> Component {
    ActionRow::buttons(buttons).into()
}

pub fn menu(select: SelectMenu) -> Component {
    ActionRow::select(select).into()
}

pub fn file(file_name: &str) -> Component {
    FileAttachment::new(file_name).into()
}

pub fn banner(url: impl Into<String>) -> Component {
    MediaGallery::single(url).into()
}

pub fn section_button(body: Vec<String>, button: Button) -> Component {
    Section::new(body, button).into()
}

pub fn section_thumb(body: Vec<String>, url: impl Into<String>) -> Component {
    Section::new(body, Thumbnail::new(url)).into()
}

pub fn panel(accent: u32, components: Vec<Component>) -> Vec<Component> {
    vec![Container::new(components).accent(accent).into()]
}

pub fn header(theme: &Theme, heading: &str, tagline: Option<&str>) -> Component {
    let mut body = vec![title(heading)];
    if let Some(tagline) = tagline {
        body.push(note(tagline));
    }
    match &theme.logo {
        Some(logo) => section_thumb(body, logo.clone()),
        None => lines(body),
    }
}

pub fn notice(accent: u32, heading: &str, body: impl AsRef<str>) -> Vec<Component> {
    panel(
        accent,
        vec![lines(vec![subtitle(heading), body.as_ref().to_string()])],
    )
}

pub fn ok(heading: &str, body: impl AsRef<str>) -> Vec<Component> {
    notice(SUCCESS, heading, body)
}

pub fn warn(heading: &str, body: impl AsRef<str>) -> Vec<Component> {
    notice(WARNING, heading, body)
}

pub fn fail(heading: &str, body: impl AsRef<str>) -> Vec<Component> {
    notice(DANGER, heading, body)
}

pub fn info(heading: &str, body: impl AsRef<str>) -> Vec<Component> {
    notice(NEUTRAL, heading, body)
}
