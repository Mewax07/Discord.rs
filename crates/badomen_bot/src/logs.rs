use std::sync::Arc;

use discord::models::{Component, MessagePayload};
use discord::rest::RestClient;

use crate::storage::ConfigStore;
use crate::ui;
use crate::util::{format_clock, now_secs};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Debug,
    Info,
    Ready,
    Warn,
    Error,
}

impl Level {
    fn label(self) -> &'static str {
        match self {
            Level::Debug => "DEBUG",
            Level::Info => "INFO ",
            Level::Ready => "READY",
            Level::Warn => "WARN ",
            Level::Error => "ERROR",
        }
    }

    fn color(self) -> &'static str {
        match self {
            Level::Debug => "\x1b[90m",
            Level::Info => "\x1b[36m",
            Level::Ready => "\x1b[32m",
            Level::Warn => "\x1b[33m",
            Level::Error => "\x1b[31m",
        }
    }
}

pub fn console(level: Level, scope: &str, message: impl AsRef<str>) {
    let line = format!(
        "\x1b[90m{}\x1b[0m {}{}\x1b[0m \x1b[95m{:<9}\x1b[0m {}",
        format_clock(now_secs()),
        level.color(),
        level.label(),
        scope,
        message.as_ref()
    );

    if level == Level::Error || level == Level::Warn {
        eprintln!("{line}");
    } else {
        println!("{line}");
    }
}

pub fn debug(scope: &str, message: impl AsRef<str>) {
    console(Level::Debug, scope, message);
}

pub fn info(scope: &str, message: impl AsRef<str>) {
    console(Level::Info, scope, message);
}

pub fn ready(scope: &str, message: impl AsRef<str>) {
    console(Level::Ready, scope, message);
}

pub fn warn(scope: &str, message: impl AsRef<str>) {
    console(Level::Warn, scope, message);
}

pub fn error(scope: &str, message: impl AsRef<str>) {
    console(Level::Error, scope, message);
}

pub struct AuditEntry {
    category: &'static str,
    title: String,
    accent: u32,
    actor: Option<String>,
    target: Option<String>,
    fields: Vec<(String, String)>,
    detail: Option<String>,
}

impl AuditEntry {
    pub fn new(category: &'static str, title: impl Into<String>) -> Self {
        Self {
            category,
            title: title.into(),
            accent: ui::NEUTRAL,
            actor: None,
            target: None,
            fields: Vec::new(),
            detail: None,
        }
    }

    pub fn accent(mut self, accent: u32) -> Self {
        self.accent = accent;
        self
    }

    pub fn actor(mut self, user_id: &str) -> Self {
        self.actor = Some(user_id.to_string());
        self
    }

    pub fn target(mut self, text: impl Into<String>) -> Self {
        self.target = Some(text.into());
        self
    }

    pub fn field(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.fields.push((name.into(), value.into()));
        self
    }

    pub fn maybe_field(self, name: impl Into<String>, value: Option<String>) -> Self {
        match value {
            Some(value) => self.field(name, value),
            None => self,
        }
    }

    pub fn detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    fn components(&self) -> Vec<Component> {
        let mut head = vec![ui::subtitle(&self.title)];

        let mut meta = Vec::new();
        if let Some(actor) = &self.actor {
            meta.push(ui::user(actor));
        }
        if let Some(target) = &self.target {
            meta.push(target.clone());
        }
        meta.push(ui::full_date(now_secs()));
        head.push(ui::note(meta.join(" · ")));

        let mut body = vec![ui::lines(head)];

        if !self.fields.is_empty() {
            body.push(ui::divider());
            body.push(ui::lines(
                self.fields
                    .iter()
                    .map(|(name, value)| ui::kv(name, value))
                    .collect(),
            ));
        }

        if let Some(detail) = &self.detail {
            body.push(ui::divider());
            body.push(ui::text(detail.clone()));
        }

        ui::panel(self.accent, body)
    }
}

pub struct Logger {
    rest: Arc<RestClient>,
    config: Arc<ConfigStore>,
}

impl Logger {
    pub fn new(rest: Arc<RestClient>, config: Arc<ConfigStore>) -> Self {
        Self { rest, config }
    }

    fn channel_for(&self, guild_id: &str, category: &str) -> Option<String> {
        self.config
            .get(guild_id)
            .log_channel(category)
            .map(String::from)
    }

    pub fn audit(&self, guild_id: &str, entry: AuditEntry) {
        let Some(channel_id) = self.channel_for(guild_id, entry.category) else {
            return;
        };
        let payload = MessagePayload::widget(entry.components()).no_mentions();
        if let Err(e) = self.rest.create_message(&channel_id, &payload) {
            error("logs", format!("audit delivery failed: {e}"));
        }
    }

    pub fn audit_with_file(
        &self,
        guild_id: &str,
        entry: AuditEntry,
        file_name: &str,
        file_bytes: &[u8],
    ) {
        let Some(channel_id) = self.channel_for(guild_id, entry.category) else {
            return;
        };
        let mut components = entry.components();
        components.push(ui::file(file_name));
        let payload = MessagePayload::widget(components).no_mentions();
        if let Err(e) =
            self.rest
                .create_message_with_file(&channel_id, &payload, file_name, file_bytes)
        {
            error("logs", format!("transcript delivery failed: {e}"));
        }
    }
}
