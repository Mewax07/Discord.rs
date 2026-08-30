use std::sync::Arc;

use discord::commands::{CommandContext, SlashCommand};
use discord::error::Result;
use discord::models::{CommandDefinition, CommandOption, Component, MessagePayload, User};
use licensing::{IssueRequest, License, LicenseService};

use crate::logs::{self, AuditEntry, Logger};
use crate::storage::{ConfigStore, LOG_CONFIG};
use crate::ui::{self, Theme};
use crate::util::{format_duration, now_secs, truncate};

pub const PLANS: &[(&str, &str, Option<u64>)] = &[
    ("trial", "Trial · 7 days", Some(7)),
    ("monthly", "Monthly · 30 days", Some(30)),
    ("quarterly", "Quarterly · 90 days", Some(90)),
    ("yearly", "Yearly · 365 days", Some(365)),
    ("lifetime", "Lifetime · never expires", None),
];

pub fn plan_days(plan: &str) -> Option<u64> {
    PLANS
        .iter()
        .find(|(key, _, _)| *key == plan)
        .and_then(|(_, _, days)| *days)
}

pub fn plan_exists(plan: &str) -> bool {
    PLANS.iter().any(|(key, _, _)| *key == plan)
}

pub struct LicenseCommand {
    pub licenses: Arc<LicenseService>,
    pub config: Arc<ConfigStore>,
    pub logger: Arc<Logger>,
    pub product: String,
    pub owner_id: Option<String>,
}

pub fn may_issue(
    ctx: &CommandContext,
    config: &ConfigStore,
    owner_id: Option<&str>,
    guild_id: &str,
) -> bool {
    let Some(author) = ctx.author() else {
        return false;
    };

    if owner_id == Some(author.id.as_str()) {
        return true;
    }

    config
        .get(guild_id)
        .manager_role_id
        .map(|role_id| ctx.has_role(&role_id))
        .unwrap_or(false)
}

impl SlashCommand for LicenseCommand {
    fn definition(&self) -> CommandDefinition {
        let mut plan = CommandOption::string("plan", "Licence plan").required(true);
        for (key, label, _) in PLANS {
            plan = plan.choice(*label, *key);
        }

        CommandDefinition::new("license", "Issue and manage product licences")
            .guild_only()
            .required_permissions(0)
            .option(
                CommandOption::subcommand("create", "Generate a new licence key")
                    .option(plan)
                    .option(CommandOption::user("member", "Owner of the licence"))
                    .option(
                        CommandOption::integer("machines", "How many machines can use it")
                            .min_value(1)
                            .max_value(25),
                    )
                    .option(
                        CommandOption::integer("days", "Override the plan duration, in days")
                            .min_value(1)
                            .max_value(3650),
                    )
                    .option(CommandOption::string("product", "Product name"))
                    .option(CommandOption::string("note", "Internal note")),
            )
            .option(
                CommandOption::subcommand("info", "Inspect a licence")
                    .option(CommandOption::string("key", "Licence key").required(true)),
            )
            .option(
                CommandOption::subcommand("revoke", "Disable a licence")
                    .option(CommandOption::string("key", "Licence key").required(true))
                    .option(CommandOption::string("reason", "Why is it revoked")),
            )
            .option(
                CommandOption::subcommand("restore", "Re-enable a revoked licence")
                    .option(CommandOption::string("key", "Licence key").required(true)),
            )
            .option(
                CommandOption::subcommand("reset-hwid", "Unbind every machine from a licence")
                    .option(CommandOption::string("key", "Licence key").required(true)),
            )
            .option(
                CommandOption::subcommand("assign", "Attach a licence to a member")
                    .option(CommandOption::string("key", "Licence key").required(true))
                    .option(CommandOption::user("member", "New owner").required(true)),
            )
            .option(
                CommandOption::subcommand("list", "List the licences of a member")
                    .option(CommandOption::user("member", "Member").required(true)),
            )
            .option(CommandOption::subcommand(
                "stats",
                "Global licence statistics",
            ))
    }

    fn execute(&self, ctx: &CommandContext) -> Result<()> {
        let Some(guild_id) = ctx.guild_id() else {
            return ctx.reply_widget_hidden(ui::fail(
                "Server only",
                "This command can only be used inside a server.",
            ));
        };
        if !may_issue(ctx, &self.config, self.owner_id.as_deref(), guild_id) {
            return ctx.reply_widget_hidden(ui::fail(
                "Not allowed",
                "Licences are issued by the owner and the manager role only. Ask them if you need a key.",
            ));
        }

        match ctx.subcommand() {
            Some("create") => self.create(ctx, guild_id),
            Some("info") => self.info(ctx),
            Some("revoke") => self.revoke(ctx, guild_id),
            Some("restore") => self.restore(ctx, guild_id),
            Some("reset-hwid") => self.reset(ctx, guild_id),
            Some("assign") => self.assign(ctx, guild_id),
            Some("list") => self.list(ctx),
            Some("stats") => self.stats(ctx, guild_id),
            _ => ctx.reply_widget_hidden(ui::fail(
                "Unknown action",
                "This subcommand does not exist.",
            )),
        }
    }
}

impl LicenseCommand {
    fn theme(&self, guild_id: &str) -> Theme {
        Theme::from_brand(&self.config.brand(guild_id))
    }

    fn create(&self, ctx: &CommandContext, guild_id: &str) -> Result<()> {
        let Some(plan) = ctx.option_string("plan") else {
            return ctx.reply_widget_hidden(ui::fail("Missing input", "A plan is required."));
        };
        if !plan_exists(plan) {
            return ctx.reply_widget_hidden(ui::fail("Unknown plan", "This plan does not exist."));
        }

        let member = ctx.option_user("member");
        let days = ctx
            .option_integer("days")
            .map(|value| value.max(1) as u64)
            .or_else(|| plan_days(plan));
        let machines = ctx.option_integer("machines").unwrap_or(1).clamp(1, 25) as u32;
        let product = ctx
            .option_string("product")
            .map(String::from)
            .unwrap_or_else(|| self.product.clone());
        let note = ctx.option_string("note").map(|value| truncate(value, 200));

        let issued = self.licenses.issue(
            IssueRequest::new(product, plan)
                .duration(days.map(|value| value * 86_400))
                .machines(machines)
                .owner(member.map(|user| user.id.clone()))
                .note(note)
                .issued_by(ctx.author().map(|user| user.id.clone())),
        );

        let license = issued.license;
        let key = issued.key;
        let theme = self.theme(guild_id);
        let delivered = match member {
            Some(user) => self.deliver(ctx, user, &license, &key, &theme),
            None => false,
        };

        self.logger.audit(
            guild_id,
            AuditEntry::new(LOG_CONFIG, "Licence issued")
                .accent(ui::SUCCESS)
                .actor(
                    ctx.author()
                        .map(|user| user.id.as_str())
                        .unwrap_or_default(),
                )
                .field("Key", ui::code(&license.key_prefix))
                .field("Plan", license.plan.clone())
                .field("Product", license.product.clone())
                .field("Machines", license.max_activations.to_string())
                .maybe_field("Owner", member.map(|user| user.mention())),
        );
        logs::info("license", format!("issued {} ({plan})", license.key_prefix));

        let mut body = license_view(&license, &theme, Some(&key));
        body.push(ui::divider());
        body.push(ui::text(ui::note(match (member, delivered) {
            (Some(user), true) => {
                format!("The key was sent to {} in direct messages.", user.mention())
            }
            (Some(user), false) => format!(
                "{} has direct messages closed, share the key manually.",
                user.mention()
            ),
            (None, _) => "No owner attached yet, use /license assign later.".to_string(),
        })));

        ctx.reply_widget_hidden(ui::panel(theme.accent, body))
    }

    fn deliver(
        &self,
        ctx: &CommandContext,
        user: &User,
        license: &License,
        key: &str,
        theme: &Theme,
    ) -> bool {
        let mut body = vec![
            ui::subtitle(format!("Your {} licence", license.product)),
            format!("**{key}**"),
            ui::kv("Plan", license.plan.clone()),
            ui::kv("Machines", license.max_activations.to_string()),
        ];

        body.push(match license.duration_secs {
            Some(duration) => ui::kv(
                "Duration",
                format!("{} once activated", format_duration(duration)),
            ),
            None => ui::kv("Duration", "never expires"),
        });
        body.push(ui::note(
            "Paste this key in the launcher to bind it to your machine. Keep it private, it is tied to your hardware.",
        ));

        ctx.direct_message(
            &user.id,
            MessagePayload::widget(ui::panel(theme.accent, vec![ui::lines(body)])).no_mentions(),
        )
        .is_ok()
    }

    fn info(&self, ctx: &CommandContext) -> Result<()> {
        let Some(key) = ctx.option_string("key") else {
            return ctx
                .reply_widget_hidden(ui::fail("Missing input", "A licence key is required."));
        };
        let license = match self.licenses.resolve(key) {
            Ok(license) => license,
            Err(e) => return ctx.reply_widget_hidden(ui::fail("Unknown licence", e.message())),
        };

        let theme = self.theme(ctx.guild_id().unwrap_or_default());
        let mut body = license_view(&license, &theme, None);

        if license.activations.is_empty() {
            body.push(ui::divider());
            body.push(ui::text(ui::note(
                "No machine is bound to this licence yet.",
            )));
        } else {
            body.push(ui::divider());
            body.push(ui::text(
                license
                    .activations
                    .iter()
                    .map(|activation| {
                        format!(
                            "{}\n{}",
                            ui::code(truncate(&activation.hwid, 40)),
                            ui::note(format!(
                                "first seen {} · last check {} · {} checks",
                                ui::short_date(activation.first_seen),
                                ui::relative(activation.last_seen),
                                activation.checks
                            ))
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n\n"),
            ));
        }

        ctx.reply_widget_hidden(ui::panel(theme.accent, body))
    }

    fn revoke(&self, ctx: &CommandContext, guild_id: &str) -> Result<()> {
        let Some(key) = ctx.option_string("key") else {
            return ctx
                .reply_widget_hidden(ui::fail("Missing input", "A licence key is required."));
        };
        let reason = ctx
            .option_string("reason")
            .map(|value| truncate(value, 200));

        match self.licenses.revoke(key, reason.clone()) {
            Ok(license) => {
                self.logger.audit(
                    guild_id,
                    AuditEntry::new(LOG_CONFIG, "Licence revoked")
                        .accent(ui::DANGER)
                        .actor(
                            ctx.author()
                                .map(|user| user.id.as_str())
                                .unwrap_or_default(),
                        )
                        .field("Key", ui::code(&license.key_prefix))
                        .maybe_field("Owner", license.owner_id.as_ref().map(|id| ui::user(id)))
                        .maybe_field("Reason", reason),
                );
                ctx.reply_widget_hidden(ui::ok(
                    "Licence revoked",
                    format!(
                        "{} stops working on the next check, and offline tokens die with their grace window.",
                        ui::code(&license.key_prefix)
                    ),
                ))
            }
            Err(e) => ctx.reply_widget_hidden(ui::fail("Cannot revoke", e.message())),
        }
    }

    fn restore(&self, ctx: &CommandContext, guild_id: &str) -> Result<()> {
        let Some(key) = ctx.option_string("key") else {
            return ctx
                .reply_widget_hidden(ui::fail("Missing input", "A licence key is required."));
        };

        match self.licenses.restore(key) {
            Ok(license) => {
                self.logger.audit(
                    guild_id,
                    AuditEntry::new(LOG_CONFIG, "Licence restored")
                        .accent(ui::SUCCESS)
                        .actor(
                            ctx.author()
                                .map(|user| user.id.as_str())
                                .unwrap_or_default(),
                        )
                        .field("Key", ui::code(&license.key_prefix)),
                );
                ctx.reply_widget_hidden(ui::ok(
                    "Licence restored",
                    format!("{} works again.", ui::code(&license.key_prefix)),
                ))
            }
            Err(e) => ctx.reply_widget_hidden(ui::fail("Cannot restore", e.message())),
        }
    }

    fn reset(&self, ctx: &CommandContext, guild_id: &str) -> Result<()> {
        let Some(key) = ctx.option_string("key") else {
            return ctx
                .reply_widget_hidden(ui::fail("Missing input", "A licence key is required."));
        };

        match self.licenses.reset_hardware(key) {
            Ok(license) => {
                self.logger.audit(
                    guild_id,
                    AuditEntry::new(LOG_CONFIG, "Licence hardware reset")
                        .accent(ui::WARNING)
                        .actor(
                            ctx.author()
                                .map(|user| user.id.as_str())
                                .unwrap_or_default(),
                        )
                        .field("Key", ui::code(&license.key_prefix)),
                );
                ctx.reply_widget_hidden(ui::ok(
                    "Hardware unbound",
                    "The next activation binds the licence to a new machine.",
                ))
            }
            Err(e) => ctx.reply_widget_hidden(ui::fail("Cannot reset", e.message())),
        }
    }

    fn assign(&self, ctx: &CommandContext, guild_id: &str) -> Result<()> {
        let (Some(key), Some(member)) = (ctx.option_string("key"), ctx.option_user("member"))
        else {
            return ctx.reply_widget_hidden(ui::fail(
                "Missing input",
                "A licence key and a member are required.",
            ));
        };

        match self.licenses.assign(key, Some(member.id.clone())) {
            Ok(license) => {
                self.logger.audit(
                    guild_id,
                    AuditEntry::new(LOG_CONFIG, "Licence assigned")
                        .accent(ui::ACCENT)
                        .actor(
                            ctx.author()
                                .map(|user| user.id.as_str())
                                .unwrap_or_default(),
                        )
                        .field("Key", ui::code(&license.key_prefix))
                        .field("Owner", member.mention()),
                );
                ctx.reply_widget_hidden(ui::ok(
                    "Licence assigned",
                    format!(
                        "{} now belongs to {}. The full key is never stored, so it cannot be resent, issue a new one if they lost it.",
                        ui::code(&license.key_prefix),
                        member.mention()
                    ),
                ))
            }
            Err(e) => ctx.reply_widget_hidden(ui::fail("Cannot assign", e.message())),
        }
    }

    fn list(&self, ctx: &CommandContext) -> Result<()> {
        let Some(member) = ctx.option_user("member") else {
            return ctx.reply_widget_hidden(ui::fail("Missing input", "A member is required."));
        };

        let licenses = self.licenses.for_owner(&member.id);
        let theme = self.theme(ctx.guild_id().unwrap_or_default());

        if licenses.is_empty() {
            return ctx.reply_widget_hidden(ui::info(
                "No licence",
                format!("{} has no licence attached.", member.mention()),
            ));
        }

        let now = now_secs();
        let listing = licenses
            .iter()
            .map(|license| {
                format!(
                    "{} · **{}**\n{}",
                    ui::code(&license.key_prefix),
                    license.plan,
                    ui::note(format!(
                        "{} · {} · {}",
                        license.status(now).as_str(),
                        expiry_label(license, now),
                        format!("{} machine(s)", license.activations.len())
                    ))
                )
            })
            .collect::<Vec<_>>()
            .join("\n\n");

        ctx.reply_widget_hidden(ui::panel(
            theme.accent,
            vec![
                ui::lines(vec![
                    ui::subtitle("Licences"),
                    ui::note(format!("{} · {} entries", member.mention(), licenses.len())),
                ]),
                ui::divider(),
                ui::text(listing),
            ],
        ))
    }

    fn stats(&self, ctx: &CommandContext, guild_id: &str) -> Result<()> {
        let stats = self.licenses.stats();
        let theme = self.theme(guild_id);

        ctx.reply_widget_hidden(ui::panel(
            theme.accent,
            vec![
                ui::lines(vec![
                    ui::subtitle("Licence statistics"),
                    ui::note(format!("{} · signed with ed25519", self.product)),
                ]),
                ui::divider(),
                ui::lines(vec![
                    ui::kv("Total", stats.total.to_string()),
                    ui::kv("Active", stats.active.to_string()),
                    ui::kv("Never used", stats.unused.to_string()),
                    ui::kv("Expired", stats.expired.to_string()),
                    ui::kv("Revoked", stats.revoked.to_string()),
                    ui::kv("Bound machines", stats.machines.to_string()),
                ]),
                ui::divider(),
                ui::text(ui::note(format!(
                    "Public key · {}",
                    ui::code(truncate(&self.licenses.public_key_hex(), 32))
                ))),
            ],
        ))
    }
}

fn expiry_label(license: &License, now: u64) -> String {
    match (license.expires_at, license.duration_secs) {
        (Some(deadline), _) if deadline <= now => "expired".to_string(),
        (Some(deadline), _) => format!("expires {}", ui::relative(deadline)),
        (None, Some(duration)) => format!("{} once activated", format_duration(duration)),
        (None, None) => "never expires".to_string(),
    }
}

fn license_view(license: &License, theme: &Theme, plaintext: Option<&str>) -> Vec<Component> {
    let now = now_secs();
    let status = license.status(now);

    let head = match plaintext {
        Some(key) => vec![
            ui::subtitle("Licence created"),
            format!("**{key}**"),
            ui::note(format!(
                "{} · {} · copy it now, only the prefix is stored",
                license.product, theme.name
            )),
        ],
        None => vec![
            ui::subtitle("Licence"),
            format!("**{}…**", license.key_prefix),
            ui::note(format!(
                "{} · {} · the full key is hashed and cannot be displayed again",
                license.product, theme.name
            )),
        ],
    };

    vec![
        ui::lines(head),
        ui::divider(),
        ui::lines(vec![
            ui::kv("Status", status.as_str()),
            ui::kv("Plan", license.plan.clone()),
            ui::kv("Validity", expiry_label(license, now)),
            ui::kv(
                "Machines",
                format!(
                    "{} of {}",
                    license.activations.len(),
                    license.max_activations
                ),
            ),
            ui::kv(
                "Owner",
                license
                    .owner_id
                    .as_ref()
                    .map(|id| ui::user(id))
                    .unwrap_or_else(ui::unset),
            ),
            ui::kv("Created", ui::short_date(license.created_at)),
        ]),
    ]
}
