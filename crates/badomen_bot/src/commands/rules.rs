use std::sync::Arc;

use discord::commands::{CommandContext, ComponentHandler, SlashCommand};
use discord::error::Result;
use discord::models::{
    Button, ButtonStyle, CommandDefinition, CommandOption, Component, MessagePayload,
    CHANNEL_TYPE_GUILD_TEXT, PERM_MANAGE_GUILD,
};

use crate::logs::{self, AuditEntry, Logger};
use crate::storage::{ConfigStore, GuildConfig, RuleEntry, LOG_CONFIG, LOG_MEMBERS};
use crate::ui::{self, Theme};
use crate::util::{now_secs, single_line, truncate};

const ACCEPT_ID: &str = "rules_accept";
const MAX_RULES: usize = 25;

fn defaults() -> Vec<RuleEntry> {
    vec![
        RuleEntry {
            title: "Respect everyone".to_string(),
            body: "No harassment, hate speech, discrimination or personal attacks. Disagree with the idea, never with the person.".to_string(),
        },
        RuleEntry {
            title: "Keep the server clean".to_string(),
            body: "No spam, no mass mentions, no advertising or DM soliciting without staff approval.".to_string(),
        },
        RuleEntry {
            title: "Safe for work only".to_string(),
            body: "NSFW, gore, shock content and illegal material are not allowed anywhere, including avatars and nicknames.".to_string(),
        },
        RuleEntry {
            title: "Use the right channel".to_string(),
            body: "Support questions belong in a ticket, feedback in the dedicated channels, off topic in the general chat.".to_string(),
        },
        RuleEntry {
            title: "No cheating or leaks".to_string(),
            body: "Sharing cracked builds, licence bypasses or private beta material results in an immediate ban.".to_string(),
        },
        RuleEntry {
            title: "Follow the staff".to_string(),
            body: "Staff decisions apply immediately. If you disagree, open a ticket instead of arguing in public.".to_string(),
        },
    ]
}

pub struct RulesCommand {
    pub config: Arc<ConfigStore>,
    pub logger: Arc<Logger>,
}

impl SlashCommand for RulesCommand {
    fn definition(&self) -> CommandDefinition {
        CommandDefinition::new("rules", "Manage and publish the server rules")
            .guild_only()
            .required_permissions(PERM_MANAGE_GUILD)
            .option(
                CommandOption::subcommand("publish", "Publish or refresh the rules panel").option(
                    CommandOption::channel(
                        "channel",
                        "Target channel, defaults to the configured one",
                    )
                    .channel_types(vec![CHANNEL_TYPE_GUILD_TEXT]),
                ),
            )
            .option(CommandOption::subcommand(
                "preview",
                "See the panel privately before publishing",
            ))
            .option(CommandOption::subcommand("list", "List the current rules"))
            .option(
                CommandOption::subcommand("add", "Append a rule")
                    .option(CommandOption::string("title", "Short rule title").required(true))
                    .option(CommandOption::string("body", "Rule details").required(true)),
            )
            .option(
                CommandOption::subcommand("edit", "Rewrite an existing rule")
                    .option(
                        CommandOption::integer("position", "Rule number")
                            .required(true)
                            .min_value(1),
                    )
                    .option(CommandOption::string("title", "New title"))
                    .option(CommandOption::string("body", "New details")),
            )
            .option(
                CommandOption::subcommand("remove", "Delete a rule").option(
                    CommandOption::integer("position", "Rule number")
                        .required(true)
                        .min_value(1),
                ),
            )
            .option(
                CommandOption::subcommand("move", "Reorder a rule")
                    .option(
                        CommandOption::integer("position", "Current rule number")
                            .required(true)
                            .min_value(1),
                    )
                    .option(
                        CommandOption::integer("to", "New rule number")
                            .required(true)
                            .min_value(1),
                    ),
            )
            .option(CommandOption::subcommand(
                "defaults",
                "Replace every rule with the default set",
            ))
    }

    fn execute(&self, ctx: &CommandContext) -> Result<()> {
        let Some(guild_id) = ctx.guild_id() else {
            return ctx.reply_widget_hidden(ui::fail(
                "Server only",
                "This command can only be used inside a server.",
            ));
        };

        match ctx.subcommand() {
            Some("publish") => self.publish(ctx, guild_id),
            Some("preview") => self.preview(ctx, guild_id),
            Some("list") => self.list(ctx, guild_id),
            Some("add") => self.add(ctx, guild_id),
            Some("edit") => self.edit(ctx, guild_id),
            Some("remove") => self.remove(ctx, guild_id),
            Some("move") => self.reorder(ctx, guild_id),
            Some("defaults") => self.defaults(ctx, guild_id),
            _ => ctx.reply_widget_hidden(ui::fail(
                "Unknown action",
                "This subcommand does not exist.",
            )),
        }
    }
}

impl RulesCommand {
    fn rules_of(&self, guild_id: &str) -> Vec<RuleEntry> {
        let cfg = self.config.get(guild_id);
        if cfg.rules.is_empty() {
            defaults()
        } else {
            cfg.rules
        }
    }

    fn touch(&self, guild_id: &str) {
        self.config
            .update(guild_id, |c| c.rules_updated_at = now_secs());
    }

    fn publish(&self, ctx: &CommandContext, guild_id: &str) -> Result<()> {
        let cfg = self.config.get(guild_id);
        let target = ctx
            .option_channel("channel")
            .map(|c| c.id.clone())
            .or_else(|| cfg.rules_channel_id.clone())
            .or_else(|| ctx.channel_id().map(String::from));

        let Some(channel_id) = target else {
            return ctx.reply_widget_hidden(ui::fail(
                "No channel",
                "Pick a channel or set one with /config rules channel.",
            ));
        };

        let theme = Theme::from_brand(&cfg.brand);
        let payload =
            MessagePayload::widget(panel(&self.rules_of(guild_id), &cfg, &theme)).no_mentions();

        let reuse = cfg
            .rules_message_id
            .as_ref()
            .filter(|_| cfg.rules_channel_id.as_deref() == Some(channel_id.as_str()));

        let message = match reuse {
            Some(message_id) => ctx
                .edit(&channel_id, message_id, payload.clone())
                .or_else(|_| ctx.send(&channel_id, payload))?,
            None => ctx.send(&channel_id, payload)?,
        };

        let (stored_channel, stored_message) = (channel_id.clone(), message.id.clone());
        self.config.update(guild_id, |c| {
            c.rules_channel_id = Some(stored_channel);
            c.rules_message_id = Some(stored_message);
        });

        if let Some(author) = ctx.author() {
            self.logger.audit(
                guild_id,
                AuditEntry::new(LOG_CONFIG, "Rules panel published")
                    .accent(theme.accent)
                    .actor(&author.id)
                    .target(ui::channel(&channel_id))
                    .field("Rules", self.rules_of(guild_id).len().to_string()),
            );
        }
        logs::info("rules", format!("panel published in {channel_id}"));

        ctx.reply_widget_hidden(ui::ok(
            "Rules published",
            format!("The panel is live in {}.", ui::channel(&channel_id)),
        ))
    }

    fn preview(&self, ctx: &CommandContext, guild_id: &str) -> Result<()> {
        let cfg = self.config.get(guild_id);
        let theme = Theme::from_brand(&cfg.brand);
        ctx.reply(
            MessagePayload::widget(panel(&self.rules_of(guild_id), &cfg, &theme))
                .ephemeral()
                .no_mentions(),
        )
    }

    fn list(&self, ctx: &CommandContext, guild_id: &str) -> Result<()> {
        let cfg = self.config.get(guild_id);
        let rules = self.rules_of(guild_id);
        let stored = !cfg.rules.is_empty();

        let body: Vec<String> = rules
            .iter()
            .enumerate()
            .map(|(index, rule)| {
                format!(
                    "**{}.** {} — {}",
                    index + 1,
                    rule.title,
                    truncate(&single_line(&rule.body), 90)
                )
            })
            .collect();

        let mut components = vec![ui::lines(vec![
            ui::subtitle("Rules content"),
            ui::note(if stored {
                format!("{} stored rules", rules.len())
            } else {
                "No rule stored yet, the default set is used".to_string()
            }),
        ])];
        components.push(ui::divider());
        components.push(ui::text(body.join("\n")));

        ctx.reply_widget_hidden(ui::panel(ui::NEUTRAL, components))
    }

    fn add(&self, ctx: &CommandContext, guild_id: &str) -> Result<()> {
        let (Some(title), Some(body)) = (ctx.option_string("title"), ctx.option_string("body"))
        else {
            return ctx.reply_widget_hidden(ui::fail(
                "Missing input",
                "A title and a body are required.",
            ));
        };

        let existing = self.config.get(guild_id).rules;
        if existing.len() >= MAX_RULES {
            return ctx.reply_widget_hidden(ui::fail(
                "Too many rules",
                format!("A server cannot hold more than **{MAX_RULES}** rules."),
            ));
        }

        let entry = RuleEntry {
            title: truncate(title, 80),
            body: truncate(body, 500),
        };

        self.config.update(guild_id, |c| {
            if c.rules.is_empty() {
                c.rules = defaults();
            }
            c.rules.push(entry);
        });
        self.touch(guild_id);
        let position = self.config.get(guild_id).rules.len();

        ctx.reply_widget_hidden(ui::ok(
            "Rule added",
            format!(
                "Stored as rule **{position}**. Run {} to refresh the panel.",
                ui::code("/rules publish")
            ),
        ))
    }

    fn edit(&self, ctx: &CommandContext, guild_id: &str) -> Result<()> {
        let Some(position) = ctx.option_integer("position") else {
            return ctx
                .reply_widget_hidden(ui::fail("Missing input", "A rule number is required."));
        };
        let index = (position.max(1) - 1) as usize;
        let title = ctx.option_string("title").map(|t| truncate(t, 80));
        let body = ctx.option_string("body").map(|b| truncate(b, 500));

        if title.is_none() && body.is_none() {
            return ctx.reply_widget_hidden(ui::fail(
                "Nothing to change",
                "Provide a new title, a new body, or both.",
            ));
        }

        let rules = self.config.get(guild_id).rules;
        if index >= rules.len() {
            return ctx.reply_widget_hidden(ui::fail(
                "Unknown rule",
                format!("There is no rule number **{position}**."),
            ));
        }

        self.config.update(guild_id, |c| {
            if let Some(rule) = c.rules.get_mut(index) {
                if let Some(title) = title {
                    rule.title = title;
                }
                if let Some(body) = body {
                    rule.body = body;
                }
            }
        });
        self.touch(guild_id);

        ctx.reply_widget_hidden(ui::ok(
            "Rule updated",
            format!("Rule **{position}** has been rewritten."),
        ))
    }

    fn remove(&self, ctx: &CommandContext, guild_id: &str) -> Result<()> {
        let Some(position) = ctx.option_integer("position") else {
            return ctx
                .reply_widget_hidden(ui::fail("Missing input", "A rule number is required."));
        };
        let index = (position.max(1) - 1) as usize;
        let rules = self.config.get(guild_id).rules;

        if index >= rules.len() {
            return ctx.reply_widget_hidden(ui::fail(
                "Unknown rule",
                format!("There is no rule number **{position}**."),
            ));
        }

        let removed = rules[index].title.clone();
        self.config.update(guild_id, |c| {
            c.rules.remove(index);
        });
        self.touch(guild_id);

        ctx.reply_widget_hidden(ui::ok(
            "Rule removed",
            format!("**{removed}** is no longer part of the rules."),
        ))
    }

    fn reorder(&self, ctx: &CommandContext, guild_id: &str) -> Result<()> {
        let (Some(from), Some(to)) = (ctx.option_integer("position"), ctx.option_integer("to"))
        else {
            return ctx
                .reply_widget_hidden(ui::fail("Missing input", "Both positions are required."));
        };

        let rules = self.config.get(guild_id).rules;
        let from_index = (from.max(1) - 1) as usize;
        let to_index = (to.max(1) - 1) as usize;

        if from_index >= rules.len() || to_index >= rules.len() {
            return ctx.reply_widget_hidden(ui::fail(
                "Unknown rule",
                format!("Positions must be between **1** and **{}**.", rules.len()),
            ));
        }

        self.config.update(guild_id, |c| {
            let rule = c.rules.remove(from_index);
            c.rules.insert(to_index, rule);
        });
        self.touch(guild_id);

        ctx.reply_widget_hidden(ui::ok(
            "Rules reordered",
            format!("Rule **{from}** is now rule **{to}**."),
        ))
    }

    fn defaults(&self, ctx: &CommandContext, guild_id: &str) -> Result<()> {
        let set = defaults();
        let count = set.len();
        self.config.update(guild_id, |c| c.rules = set);
        self.touch(guild_id);

        ctx.reply_widget_hidden(ui::ok(
            "Defaults restored",
            format!(
                "**{count}** rules are ready. Run {} to update the panel.",
                ui::code("/rules publish")
            ),
        ))
    }
}

fn panel(rules: &[RuleEntry], cfg: &GuildConfig, theme: &Theme) -> Vec<Component> {
    let mut body = vec![ui::header(
        theme,
        &format!("{} · Server rules", theme.name),
        Some("Read everything below, then unlock the server with the button at the bottom."),
    )];

    body.push(ui::divider());

    let mut block = String::new();
    let mut written = 0usize;
    let mut skipped = 0usize;

    for (index, rule) in rules.iter().enumerate() {
        let piece = format!("**{}. {}**\n{}\n\n", index + 1, rule.title, rule.body);
        if written + piece.len() > 3_000 {
            skipped += 1;
            continue;
        }
        written += piece.len();
        if block.len() + piece.len() > 1_400 {
            body.push(ui::text(block.trim_end().to_string()));
            body.push(ui::gap());
            block = String::new();
        }
        block.push_str(&piece);
    }
    if !block.is_empty() {
        body.push(ui::text(block.trim_end().to_string()));
    }
    if skipped > 0 {
        body.push(ui::text(ui::note(format!(
            "{skipped} additional rules could not fit in this panel."
        ))));
    }

    body.push(ui::divider());

    let unlock = match &cfg.member_role_id {
        Some(role_id) => format!(
            "Accepting grants you {} and opens the rest of the server.",
            ui::role(role_id)
        ),
        None => "Accepting confirms you have read every rule above.".to_string(),
    };

    body.push(ui::section_button(
        vec![ui::subtitle("Ready to join"), unlock],
        Button::new("I accept the rules", ACCEPT_ID, ButtonStyle::Success),
    ));

    let updated = if cfg.rules_updated_at > 0 {
        format!("Last update {}", ui::short_date(cfg.rules_updated_at))
    } else {
        "Standard rule set".to_string()
    };
    body.push(ui::text(ui::note(format!("{} · {updated}", theme.footer))));

    ui::panel(theme.accent, body)
}

pub struct RulesAcceptHandler {
    pub config: Arc<ConfigStore>,
    pub logger: Arc<Logger>,
}

impl ComponentHandler for RulesAcceptHandler {
    fn matches(&self, custom_id: &str) -> bool {
        custom_id == ACCEPT_ID
    }

    fn execute(&self, ctx: &CommandContext) -> Result<()> {
        let Some(guild_id) = ctx.guild_id() else {
            return ctx.reply_widget_hidden(ui::fail(
                "Server only",
                "This button only works inside a server.",
            ));
        };
        let Some(author) = ctx.author() else {
            return ctx.reply_widget_hidden(ui::fail("Unknown user", "Unable to identify you."));
        };

        let cfg = self.config.get(guild_id);
        let Some(role_id) = cfg.member_role_id.clone() else {
            return ctx.reply_widget_hidden(ui::warn(
                "Not configured yet",
                format!(
                    "No member role is set. An admin has to run {} first.",
                    ui::code("/config rules member-role")
                ),
            ));
        };

        if ctx.has_role(&role_id) {
            return ctx.reply_widget_hidden(ui::info(
                "Already unlocked",
                format!("You already have {}, welcome back.", ui::role(&role_id)),
            ));
        }

        match ctx.add_role(guild_id, &author.id, &role_id) {
            Ok(()) => {
                self.logger.audit(
                    guild_id,
                    AuditEntry::new(LOG_MEMBERS, "Rules accepted")
                        .accent(ui::SUCCESS)
                        .actor(&author.id)
                        .field("Granted", ui::role(&role_id)),
                );
                ctx.reply_widget_hidden(ui::ok(
                    "Welcome aboard",
                    format!(
                        "{} is yours. Have a look around and enjoy your stay.",
                        ui::role(&role_id)
                    ),
                ))
            }
            Err(e) => {
                logs::error(
                    "rules",
                    format!(
                        "role grant failed ({e}) - the bot role must sit above {} in the role list and hold Manage Roles",
                        role_id
                    ),
                );
                ctx.reply_widget_hidden(ui::fail(
                    "Role not granted",
                    "The bot cannot hand out this role yet. Staff: move the bot role above the member role and give it the Manage Roles permission.",
                ))
            }
        }
    }
}
