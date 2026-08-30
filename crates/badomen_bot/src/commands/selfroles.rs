use std::sync::Arc;

use discord::commands::{CommandContext, ComponentHandler, SlashCommand};
use discord::error::Result;
use discord::models::{
    Button, ButtonStyle, CommandDefinition, CommandOption, Component, MessagePayload, SelectMenu,
    SelectOption, CHANNEL_TYPE_GUILD_TEXT, PERM_MANAGE_GUILD,
};

use crate::logs::{AuditEntry, Logger};
use crate::storage::{ConfigStore, LOG_MEMBERS};
use crate::ui::{self, Theme};

const SELECT_ID: &str = "selfroles_select";
const CLEAR_ID: &str = "selfroles_clear";

pub struct SelfRole {
    pub key: &'static str,
    pub label: &'static str,
    pub description: &'static str,
}

pub const CATALOG: &[SelfRole] = &[
    SelfRole {
        key: "dev_logs",
        label: "Dev logs",
        description: "Behind the scenes progress on every project",
    },
    SelfRole {
        key: "updates",
        label: "Updates",
        description: "Pings when a new version ships",
    },
    SelfRole {
        key: "restarts",
        label: "Restarts",
        description: "Heads up before a scheduled maintenance",
    },
    SelfRole {
        key: "badomen",
        label: "BadOmen Visuals",
        description: "News about BadOmen Visuals",
    },
    SelfRole {
        key: "nouga",
        label: "Nouga Launcher",
        description: "News about Nouga Launcher",
    },
    SelfRole {
        key: "norvoro",
        label: "Norvoro Server",
        description: "News about Norvoro Server",
    },
];

pub struct SelfRolesCommand {
    pub config: Arc<ConfigStore>,
}

impl SlashCommand for SelfRolesCommand {
    fn definition(&self) -> CommandDefinition {
        CommandDefinition::new("selfroles", "Publish the notification roles panel")
            .guild_only()
            .required_permissions(PERM_MANAGE_GUILD)
            .option(
                CommandOption::channel("channel", "Target channel, defaults to the current one")
                    .channel_types(vec![CHANNEL_TYPE_GUILD_TEXT]),
            )
    }

    fn execute(&self, ctx: &CommandContext) -> Result<()> {
        let Some(guild_id) = ctx.guild_id() else {
            return ctx.reply_widget_hidden(ui::fail(
                "Server only",
                "This command can only be used inside a server.",
            ));
        };
        let target = ctx
            .option_channel("channel")
            .map(|c| c.id.clone())
            .or_else(|| ctx.channel_id().map(String::from));
        let Some(channel_id) = target else {
            return ctx.reply_widget_hidden(ui::fail("No channel", "No channel context."));
        };

        let cfg = self.config.get(guild_id);
        let available: Vec<&SelfRole> = CATALOG
            .iter()
            .filter(|role| cfg.self_roles.contains_key(role.key))
            .collect();

        if available.is_empty() {
            return ctx.reply_widget_hidden(ui::warn(
                "Nothing to show",
                format!(
                    "Map at least one role with {} before publishing the panel.",
                    ui::code("/config selfroles set")
                ),
            ));
        }

        let theme = Theme::from_brand(&cfg.brand);
        ctx.send(
            &channel_id,
            MessagePayload::widget(panel(&available, &cfg.self_roles, &theme)).no_mentions(),
        )?;

        ctx.reply_widget_hidden(ui::ok(
            "Panel published",
            format!(
                "{} roles are available in {}.",
                available.len(),
                ui::channel(&channel_id)
            ),
        ))
    }
}

fn panel(
    available: &[&SelfRole],
    mapping: &std::collections::HashMap<String, String>,
    theme: &Theme,
) -> Vec<Component> {
    let listing: Vec<String> = available
        .iter()
        .map(|role| {
            let mention = mapping
                .get(role.key)
                .map(|id| ui::role(id))
                .unwrap_or_default();
            format!(
                "**{}** · {mention}\n{}",
                role.label,
                ui::note(role.description)
            )
        })
        .collect();

    let options: Vec<SelectOption> = available
        .iter()
        .map(|role| SelectOption::new(role.label, role.key).description(role.description))
        .collect();

    let menu = SelectMenu::new(SELECT_ID, options.clone())
        .placeholder("Select every notification you want")
        .multi(0, options.len() as u32);

    ui::panel(
        theme.accent,
        vec![
            ui::header(
                theme,
                "Notification roles",
                Some("Your selection is applied instantly, unselect to remove a role."),
            ),
            ui::divider(),
            ui::text(listing.join("\n\n")),
            ui::divider(),
            ui::menu(menu),
            ui::row(vec![Button::new(
                "Remove every role",
                CLEAR_ID,
                ButtonStyle::Secondary,
            )]),
            ui::text(ui::note(&theme.footer)),
        ],
    )
}

pub struct SelfRolesSelectHandler {
    pub config: Arc<ConfigStore>,
    pub logger: Arc<Logger>,
}

impl ComponentHandler for SelfRolesSelectHandler {
    fn matches(&self, custom_id: &str) -> bool {
        custom_id == SELECT_ID || custom_id == CLEAR_ID
    }

    fn execute(&self, ctx: &CommandContext) -> Result<()> {
        let Some(guild_id) = ctx.guild_id() else {
            return ctx.reply_widget_hidden(ui::fail(
                "Server only",
                "This panel only works inside a server.",
            ));
        };
        let Some(author) = ctx.author() else {
            return ctx.reply_widget_hidden(ui::fail("Unknown user", "Unable to identify you."));
        };

        let clearing = ctx.custom_id() == Some(CLEAR_ID);
        let cfg = self.config.get(guild_id);
        let selected: Vec<&str> = if clearing {
            Vec::new()
        } else {
            ctx.selected_values().iter().map(String::as_str).collect()
        };

        let mut added = Vec::new();
        let mut removed = Vec::new();
        let mut failed = 0usize;

        for role in CATALOG {
            let Some(role_id) = cfg.self_roles.get(role.key) else {
                continue;
            };
            let holds = ctx.has_role(role_id);
            let wants = selected.contains(&role.key);

            if wants && !holds {
                match ctx.add_role(guild_id, &author.id, role_id) {
                    Ok(()) => added.push(role.label),
                    Err(_) => failed += 1,
                }
            } else if !wants && holds {
                match ctx.remove_role(guild_id, &author.id, role_id) {
                    Ok(()) => removed.push(role.label),
                    Err(_) => failed += 1,
                }
            }
        }

        if added.is_empty() && removed.is_empty() {
            let body = if failed > 0 {
                "The role changes could not be applied, please contact the staff.".to_string()
            } else {
                "Your notification roles are already up to date.".to_string()
            };
            return ctx.reply_widget_hidden(ui::info("Nothing changed", body));
        }

        let mut summary = Vec::new();
        if !added.is_empty() {
            summary.push(ui::kv("Added", added.join(", ")));
        }
        if !removed.is_empty() {
            summary.push(ui::kv("Removed", removed.join(", ")));
        }
        if failed > 0 {
            summary.push(ui::kv("Failed", failed.to_string()));
        }

        self.logger.audit(
            guild_id,
            AuditEntry::new(LOG_MEMBERS, "Notification roles updated")
                .accent(ui::ACCENT)
                .actor(&author.id)
                .maybe_field("Added", (!added.is_empty()).then(|| added.join(", ")))
                .maybe_field("Removed", (!removed.is_empty()).then(|| removed.join(", "))),
        );

        ctx.reply_widget_hidden(ui::panel(
            ui::SUCCESS,
            vec![ui::lines(
                std::iter::once(ui::subtitle("Roles updated"))
                    .chain(summary)
                    .collect(),
            )],
        ))
    }
}
