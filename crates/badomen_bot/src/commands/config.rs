use std::sync::Arc;

use discord::commands::{CommandContext, SlashCommand};
use discord::error::Result;
use discord::models::{
    CommandDefinition, CommandOption, Component, CHANNEL_TYPE_GUILD_CATEGORY,
    CHANNEL_TYPE_GUILD_TEXT, PERM_MANAGE_GUILD,
};

use crate::commands::selfroles::CATALOG;
use crate::commands::ticket::CATEGORIES;
use crate::logs::{AuditEntry, Logger};
use crate::storage::{ConfigStore, GuildConfig, LOG_CONFIG, LOG_KEYS};
use crate::ui::{self, Theme};

pub struct ConfigCommand {
    pub config: Arc<ConfigStore>,
    pub logger: Arc<Logger>,
}

impl SlashCommand for ConfigCommand {
    fn definition(&self) -> CommandDefinition {
        let mut ticket_category =
            CommandOption::string("category", "Ticket category").required(true);
        for category in CATEGORIES {
            ticket_category = ticket_category.choice(category.label, category.key);
        }

        let mut self_role = CommandOption::string("role_key", "Panel entry").required(true);
        for role in CATALOG {
            self_role = self_role.choice(role.label, role.key);
        }

        let mut log_kind = CommandOption::string("kind", "Log category").required(true);
        for (key, description) in LOG_KEYS {
            log_kind = log_kind.choice(format!("{key} — {description}"), *key);
        }

        CommandDefinition::new("config", "Configure every system of the bot")
            .guild_only()
            .required_permissions(PERM_MANAGE_GUILD)
            .option(CommandOption::subcommand(
                "view",
                "Show the full configuration of this server",
            ))
            .option(
                CommandOption::group("tickets", "Ticket system settings")
                    .option(
                        CommandOption::subcommand("category", "Category holding ticket channels")
                            .option(
                                CommandOption::channel("channel", "Discord category")
                                    .required(true)
                                    .channel_types(vec![CHANNEL_TYPE_GUILD_CATEGORY]),
                            ),
                    )
                    .option(
                        CommandOption::subcommand("staff-role", "Role with access to every ticket")
                            .option(CommandOption::role("role", "Staff role").required(true)),
                    )
                    .option(
                        CommandOption::subcommand("ping", "Role pinged for one ticket category")
                            .option(ticket_category)
                            .option(CommandOption::role("role", "Role to ping").required(true)),
                    )
                    .option(
                        CommandOption::subcommand(
                            "transcripts",
                            "Attach a transcript when closing",
                        )
                        .option(
                            CommandOption::boolean("enabled", "Enable transcripts").required(true),
                        ),
                    )
                    .option(
                        CommandOption::subcommand(
                            "dm-summary",
                            "Send a recap to the ticket author",
                        )
                        .option(
                            CommandOption::boolean("enabled", "Enable the recap").required(true),
                        ),
                    ),
            )
            .option(
                CommandOption::group("rules", "Rules panel wiring")
                    .option(
                        CommandOption::subcommand("channel", "Channel holding the rules panel")
                            .option(
                                CommandOption::channel("channel", "Rules channel")
                                    .required(true)
                                    .channel_types(vec![CHANNEL_TYPE_GUILD_TEXT]),
                            ),
                    )
                    .option(
                        CommandOption::subcommand("member-role", "Role granted on acceptance")
                            .option(CommandOption::role("role", "Member role").required(true)),
                    ),
            )
            .option(
                CommandOption::group("licensing", "Who is allowed to issue licence keys").option(
                    CommandOption::subcommand(
                        "manager-role",
                        "Role allowed to issue keys alongside the owner",
                    )
                    .option(CommandOption::role("role", "Manager role").required(true)),
                ),
            )
            .option(
                CommandOption::group("logs", "Where each log category is delivered")
                    .option(
                        CommandOption::subcommand("set", "Route a log category to a channel")
                            .option(log_kind.clone())
                            .option(
                                CommandOption::channel("channel", "Target channel")
                                    .required(true)
                                    .channel_types(vec![CHANNEL_TYPE_GUILD_TEXT]),
                            ),
                    )
                    .option(
                        CommandOption::subcommand("clear", "Stop delivering a log category")
                            .option(log_kind),
                    ),
            )
            .option(
                CommandOption::group("selfroles", "Notification roles mapping")
                    .option(
                        CommandOption::subcommand("set", "Map a panel entry to a real role")
                            .option(self_role.clone())
                            .option(CommandOption::role("role", "Role to assign").required(true)),
                    )
                    .option(
                        CommandOption::subcommand("clear", "Remove a panel entry")
                            .option(self_role),
                    ),
            )
            .option(
                CommandOption::group("brand", "Look and feel of every widget")
                    .option(
                        CommandOption::subcommand("name", "Displayed brand name")
                            .option(CommandOption::string("value", "Brand name").required(true)),
                    )
                    .option(
                        CommandOption::subcommand("accent", "Accent colour of every widget")
                            .option(
                                CommandOption::string("hex", "Colour such as #8B5CF6")
                                    .required(true),
                            ),
                    )
                    .option(
                        CommandOption::subcommand("logo", "Small logo shown in panels").option(
                            CommandOption::string("url", "Direct image URL").required(true),
                        ),
                    )
                    .option(
                        CommandOption::subcommand("banner", "Wide banner shown in panels").option(
                            CommandOption::string("url", "Direct image URL").required(true),
                        ),
                    )
                    .option(
                        CommandOption::subcommand("footer", "Footer line of every widget")
                            .option(CommandOption::string("value", "Footer text").required(true)),
                    )
                    .option(CommandOption::subcommand(
                        "reset",
                        "Back to the default look",
                    )),
            )
    }

    fn execute(&self, ctx: &CommandContext) -> Result<()> {
        let Some(guild_id) = ctx.guild_id() else {
            return ctx.reply_widget_hidden(ui::fail(
                "Server only",
                "This command can only be used inside a server.",
            ));
        };

        match ctx.route() {
            (None, Some("view")) => self.view(ctx, guild_id),
            (Some("tickets"), Some(action)) => self.tickets(ctx, guild_id, action),
            (Some("rules"), Some(action)) => self.rules(ctx, guild_id, action),
            (Some("licensing"), Some("manager-role")) => self.manager_role(ctx, guild_id),
            (Some("logs"), Some(action)) => self.logs(ctx, guild_id, action),
            (Some("selfroles"), Some(action)) => self.selfroles(ctx, guild_id, action),
            (Some("brand"), Some(action)) => self.brand(ctx, guild_id, action),
            _ => ctx.reply_widget_hidden(ui::fail(
                "Unknown action",
                "This subcommand does not exist.",
            )),
        }
    }
}

impl ConfigCommand {
    fn saved(
        &self,
        ctx: &CommandContext,
        guild_id: &str,
        title: &str,
        summary: String,
    ) -> Result<()> {
        if let Some(author) = ctx.author() {
            self.logger.audit(
                guild_id,
                AuditEntry::new(LOG_CONFIG, format!("Configuration · {title}"))
                    .accent(ui::ACCENT)
                    .actor(&author.id)
                    .detail(summary.clone()),
            );
        }
        ctx.reply_widget_hidden(ui::ok(title, summary))
    }

    fn view(&self, ctx: &CommandContext, guild_id: &str) -> Result<()> {
        let cfg = self.config.get(guild_id);
        let theme = Theme::from_brand(&cfg.brand);

        let tickets = vec![
            ui::kv("Category", optional_channel(&cfg.ticket_category_id)),
            ui::kv("Staff role", optional_role(&cfg.staff_role_id)),
            ui::kv("Transcripts", on_off(cfg.ticket_transcripts)),
            ui::kv("Author recap", on_off(cfg.ticket_dm_summary)),
            ui::kv("Opened so far", cfg.ticket_counter.to_string()),
        ];

        let pings: Vec<String> = CATEGORIES
            .iter()
            .map(|category| {
                ui::kv(
                    category.label,
                    cfg.category_roles
                        .get(category.key)
                        .map(|id| ui::role(id))
                        .unwrap_or_else(ui::unset),
                )
            })
            .collect();

        let community = vec![
            ui::kv("Rules channel", optional_channel(&cfg.rules_channel_id)),
            ui::kv("Member role", optional_role(&cfg.member_role_id)),
            ui::kv("Licence managers", optional_role(&cfg.manager_role_id)),
            ui::kv(
                "Stored rules",
                if cfg.rules.is_empty() {
                    "default set".to_string()
                } else {
                    cfg.rules.len().to_string()
                },
            ),
        ];

        let self_roles: Vec<String> = CATALOG
            .iter()
            .map(|role| {
                ui::kv(
                    role.label,
                    cfg.self_roles
                        .get(role.key)
                        .map(|id| ui::role(id))
                        .unwrap_or_else(ui::unset),
                )
            })
            .collect();

        let logs: Vec<String> = LOG_KEYS
            .iter()
            .map(|(key, _)| {
                ui::kv(
                    *key,
                    cfg.log_channels
                        .get(*key)
                        .map(|id| ui::channel(id))
                        .unwrap_or_else(ui::unset),
                )
            })
            .collect();

        let brand = vec![
            ui::kv("Name", theme.name.clone()),
            ui::kv("Accent", format!("`#{:06X}`", theme.accent)),
            ui::kv("Logo", present(&cfg.brand.logo_url)),
            ui::kv("Banner", present(&cfg.brand.banner_url)),
            ui::kv("Footer", theme.footer.clone()),
        ];

        let components: Vec<Component> = vec![
            ui::header(
                &theme,
                "Server configuration",
                Some(&format!("Guild {guild_id}")),
            ),
            ui::divider(),
            ui::lines(prefixed("Tickets", tickets)),
            ui::gap(),
            ui::lines(prefixed("Category pings", pings)),
            ui::divider(),
            ui::lines(prefixed("Community", community)),
            ui::gap(),
            ui::lines(prefixed("Notification roles", self_roles)),
            ui::divider(),
            ui::lines(prefixed("Log routing", logs)),
            ui::gap(),
            ui::lines(prefixed("Branding", brand)),
            ui::divider(),
            ui::text(ui::note(missing_summary(&cfg))),
        ];

        ctx.reply_widget_hidden(ui::panel(theme.accent, components))
    }

    fn tickets(&self, ctx: &CommandContext, guild_id: &str, action: &str) -> Result<()> {
        match action {
            "category" => {
                let Some(channel) = ctx.option_channel("channel") else {
                    return ctx.reply_widget_hidden(ui::fail("Not found", "Channel not resolved."));
                };
                if !channel.is_category() {
                    return ctx.reply_widget_hidden(ui::fail(
                        "Wrong channel",
                        "The selected channel must be a Discord category.",
                    ));
                }
                let id = channel.id.clone();
                self.config
                    .update(guild_id, |c| c.ticket_category_id = Some(id.clone()));
                self.saved(
                    ctx,
                    guild_id,
                    "Ticket category",
                    format!(
                        "New tickets are created under {}.",
                        ui::channel(&channel.id)
                    ),
                )
            }
            "staff-role" => {
                let Some(role) = ctx.option_role("role") else {
                    return ctx.reply_widget_hidden(ui::fail("Not found", "Role not resolved."));
                };
                let id = role.id.clone();
                self.config
                    .update(guild_id, |c| c.staff_role_id = Some(id.clone()));
                self.saved(
                    ctx,
                    guild_id,
                    "Staff role",
                    format!("{} can now see and manage every ticket.", role.mention()),
                )
            }
            "ping" => {
                let (Some(category), Some(role)) =
                    (ctx.option_string("category"), ctx.option_role("role"))
                else {
                    return ctx.reply_widget_hidden(ui::fail(
                        "Missing input",
                        "A category and a role are required.",
                    ));
                };
                let (key, id) = (category.to_string(), role.id.clone());
                self.config.update(guild_id, |c| {
                    c.category_roles.insert(key.clone(), id.clone());
                });
                self.saved(
                    ctx,
                    guild_id,
                    "Category ping",
                    format!(
                        "Tickets in **{}** now ping {}.",
                        category_label(category),
                        role.mention()
                    ),
                )
            }
            "transcripts" => {
                let enabled = ctx.option_boolean("enabled").unwrap_or(true);
                self.config
                    .update(guild_id, |c| c.ticket_transcripts = enabled);
                self.saved(
                    ctx,
                    guild_id,
                    "Transcripts",
                    format!(
                        "Closing a ticket now {} a transcript.",
                        if enabled { "attaches" } else { "skips" }
                    ),
                )
            }
            "dm-summary" => {
                let enabled = ctx.option_boolean("enabled").unwrap_or(true);
                self.config
                    .update(guild_id, |c| c.ticket_dm_summary = enabled);
                self.saved(
                    ctx,
                    guild_id,
                    "Author recap",
                    format!(
                        "Ticket authors {} a private recap when their ticket closes.",
                        if enabled {
                            "receive"
                        } else {
                            "no longer receive"
                        }
                    ),
                )
            }
            _ => {
                ctx.reply_widget_hidden(ui::fail("Unknown action", "This setting does not exist."))
            }
        }
    }

    fn rules(&self, ctx: &CommandContext, guild_id: &str, action: &str) -> Result<()> {
        match action {
            "channel" => {
                let Some(channel) = ctx.option_channel("channel") else {
                    return ctx.reply_widget_hidden(ui::fail("Not found", "Channel not resolved."));
                };
                let id = channel.id.clone();
                self.config.update(guild_id, |c| {
                    c.rules_channel_id = Some(id.clone());
                    c.rules_message_id = None;
                });
                self.saved(
                    ctx,
                    guild_id,
                    "Rules channel",
                    format!(
                        "The panel will be published in {} on the next {}.",
                        channel.mention(),
                        ui::code("/rules publish")
                    ),
                )
            }
            "member-role" => {
                let Some(role) = ctx.option_role("role") else {
                    return ctx.reply_widget_hidden(ui::fail("Not found", "Role not resolved."));
                };
                let id = role.id.clone();
                self.config
                    .update(guild_id, |c| c.member_role_id = Some(id.clone()));
                self.saved(
                    ctx,
                    guild_id,
                    "Member role",
                    format!("Accepting the rules now grants {}.", role.mention()),
                )
            }
            _ => {
                ctx.reply_widget_hidden(ui::fail("Unknown action", "This setting does not exist."))
            }
        }
    }

    fn manager_role(&self, ctx: &CommandContext, guild_id: &str) -> Result<()> {
        let Some(role) = ctx.option_role("role") else {
            return ctx.reply_widget_hidden(ui::fail("Not found", "Role not resolved."));
        };
        let id = role.id.clone();
        self.config
            .update(guild_id, |c| c.manager_role_id = Some(id.clone()));
        self.saved(
            ctx,
            guild_id,
            "Manager role",
            format!(
                "{} can now issue licence keys, alongside the owner. Grant the role access to the command in Server Settings, Integrations.",
                role.mention()
            ),
        )
    }

    fn logs(&self, ctx: &CommandContext, guild_id: &str, action: &str) -> Result<()> {
        let Some(kind) = ctx.option_string("kind") else {
            return ctx
                .reply_widget_hidden(ui::fail("Missing input", "A log category is required."));
        };

        match action {
            "set" => {
                let Some(channel) = ctx.option_channel("channel") else {
                    return ctx.reply_widget_hidden(ui::fail("Not found", "Channel not resolved."));
                };
                let (key, id) = (kind.to_string(), channel.id.clone());
                self.config.update(guild_id, |c| {
                    c.log_channels.insert(key.clone(), id.clone());
                });
                self.saved(
                    ctx,
                    guild_id,
                    "Log routing",
                    format!("**{kind}** logs are delivered to {}.", channel.mention()),
                )
            }
            "clear" => {
                let key = kind.to_string();
                self.config.update(guild_id, |c| {
                    c.log_channels.remove(&key);
                });
                self.saved(
                    ctx,
                    guild_id,
                    "Log routing",
                    format!("**{kind}** logs are no longer delivered."),
                )
            }
            _ => {
                ctx.reply_widget_hidden(ui::fail("Unknown action", "This setting does not exist."))
            }
        }
    }

    fn selfroles(&self, ctx: &CommandContext, guild_id: &str, action: &str) -> Result<()> {
        let Some(key) = ctx.option_string("role_key") else {
            return ctx
                .reply_widget_hidden(ui::fail("Missing input", "A panel entry is required."));
        };
        let label = CATALOG
            .iter()
            .find(|role| role.key == key)
            .map(|role| role.label)
            .unwrap_or(key);

        match action {
            "set" => {
                let Some(role) = ctx.option_role("role") else {
                    return ctx.reply_widget_hidden(ui::fail("Not found", "Role not resolved."));
                };
                let (entry, id) = (key.to_string(), role.id.clone());
                self.config.update(guild_id, |c| {
                    c.self_roles.insert(entry.clone(), id.clone());
                });
                self.saved(
                    ctx,
                    guild_id,
                    "Notification role",
                    format!("**{label}** now assigns {}.", role.mention()),
                )
            }
            "clear" => {
                let entry = key.to_string();
                self.config.update(guild_id, |c| {
                    c.self_roles.remove(&entry);
                });
                self.saved(
                    ctx,
                    guild_id,
                    "Notification role",
                    format!("**{label}** was removed from the panel."),
                )
            }
            _ => {
                ctx.reply_widget_hidden(ui::fail("Unknown action", "This setting does not exist."))
            }
        }
    }

    fn brand(&self, ctx: &CommandContext, guild_id: &str, action: &str) -> Result<()> {
        match action {
            "name" => {
                let Some(value) = ctx.option_string("value") else {
                    return ctx
                        .reply_widget_hidden(ui::fail("Missing input", "A name is required."));
                };
                let name = value.to_string();
                self.config
                    .update(guild_id, |c| c.brand.name = Some(name.clone()));
                self.saved(
                    ctx,
                    guild_id,
                    "Brand name",
                    format!("Widgets now read **{value}**."),
                )
            }
            "accent" => {
                let Some(value) = ctx.option_string("hex") else {
                    return ctx
                        .reply_widget_hidden(ui::fail("Missing input", "A colour is required."));
                };
                let Some(color) = parse_hex(value) else {
                    return ctx.reply_widget_hidden(ui::fail(
                        "Invalid colour",
                        format!("Use a hexadecimal value such as {}.", ui::code("#8B5CF6")),
                    ));
                };
                self.config
                    .update(guild_id, |c| c.brand.accent = Some(color));
                self.saved(
                    ctx,
                    guild_id,
                    "Accent colour",
                    format!("Every widget now uses `#{color:06X}`."),
                )
            }
            "logo" => {
                let Some(url) = ctx.option_string("url") else {
                    return ctx
                        .reply_widget_hidden(ui::fail("Missing input", "A URL is required."));
                };
                if !is_http(url) {
                    return ctx.reply_widget_hidden(ui::fail(
                        "Invalid URL",
                        "The logo must be a direct http or https image link.",
                    ));
                }
                let value = url.to_string();
                self.config
                    .update(guild_id, |c| c.brand.logo_url = Some(value.clone()));
                self.saved(
                    ctx,
                    guild_id,
                    "Logo",
                    "Panels now show the new logo.".to_string(),
                )
            }
            "banner" => {
                let Some(url) = ctx.option_string("url") else {
                    return ctx
                        .reply_widget_hidden(ui::fail("Missing input", "A URL is required."));
                };
                if !is_http(url) {
                    return ctx.reply_widget_hidden(ui::fail(
                        "Invalid URL",
                        "The banner must be a direct http or https image link.",
                    ));
                }
                let value = url.to_string();
                self.config
                    .update(guild_id, |c| c.brand.banner_url = Some(value.clone()));
                self.saved(
                    ctx,
                    guild_id,
                    "Banner",
                    "Panels now show the new banner.".to_string(),
                )
            }
            "footer" => {
                let Some(value) = ctx.option_string("value") else {
                    return ctx
                        .reply_widget_hidden(ui::fail("Missing input", "A footer is required."));
                };
                let footer = value.to_string();
                self.config
                    .update(guild_id, |c| c.brand.footer = Some(footer.clone()));
                self.saved(
                    ctx,
                    guild_id,
                    "Footer",
                    format!("Widgets now end with *{value}*."),
                )
            }
            "reset" => {
                self.config
                    .update(guild_id, |c| c.brand = Default::default());
                self.saved(
                    ctx,
                    guild_id,
                    "Branding reset",
                    "Widgets are back to the default look.".to_string(),
                )
            }
            _ => {
                ctx.reply_widget_hidden(ui::fail("Unknown action", "This setting does not exist."))
            }
        }
    }
}

fn prefixed(heading: &str, values: Vec<String>) -> Vec<String> {
    std::iter::once(ui::subtitle(heading))
        .chain(values)
        .collect()
}

fn optional_channel(value: &Option<String>) -> String {
    value
        .as_ref()
        .map(|id| ui::channel(id))
        .unwrap_or_else(ui::unset)
}

fn optional_role(value: &Option<String>) -> String {
    value
        .as_ref()
        .map(|id| ui::role(id))
        .unwrap_or_else(ui::unset)
}

fn present(value: &Option<String>) -> String {
    match value {
        Some(_) => "set".to_string(),
        None => ui::unset(),
    }
}

fn on_off(value: bool) -> String {
    if value {
        "enabled".to_string()
    } else {
        "disabled".to_string()
    }
}

fn category_label(key: &str) -> &'static str {
    CATEGORIES
        .iter()
        .find(|category| category.key == key)
        .map(|category| category.label)
        .unwrap_or("Unknown category")
}

fn missing_summary(cfg: &GuildConfig) -> String {
    let mut missing = Vec::new();
    if cfg.ticket_category_id.is_none() {
        missing.push("ticket category");
    }
    if cfg.staff_role_id.is_none() {
        missing.push("staff role");
    }
    if cfg.member_role_id.is_none() {
        missing.push("member role");
    }
    if cfg.log_channels.is_empty() && cfg.ticket_log_channel_id.is_none() {
        missing.push("log routing");
    }

    if missing.is_empty() {
        "Every core setting is configured.".to_string()
    } else {
        format!("Still missing: {}.", missing.join(", "))
    }
}

fn parse_hex(value: &str) -> Option<u32> {
    let cleaned = value
        .trim()
        .trim_start_matches('#')
        .trim_start_matches("0x");
    (cleaned.len() == 6)
        .then(|| u32::from_str_radix(cleaned, 16).ok())
        .flatten()
}

fn is_http(url: &str) -> bool {
    url.starts_with("http://") || url.starts_with("https://")
}
