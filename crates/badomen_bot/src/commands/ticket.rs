use std::collections::HashSet;
use std::sync::Arc;

use discord::commands::{CommandContext, ComponentHandler, SlashCommand};
use discord::error::Result;
use discord::models::{
    ActionRow, AllowedMentions, Button, ButtonStyle, CommandDefinition, CommandOption, Component,
    MessagePayload, NewChannel, PermissionOverwrite, SelectMenu, SelectOption, TextInput,
    TextInputStyle, CHANNEL_TYPE_GUILD_TEXT, PERM_MANAGE_GUILD, PERM_VIEW_CHANNEL,
    TICKET_MEMBER_PERMS,
};
use discord::rest::RestClient;

use crate::logs::{self, AuditEntry, Logger};
use crate::scheduler::Scheduler;
use crate::storage::{ConfigStore, GuildConfig, TicketRecord, TicketStore, LOG_TICKETS};
use crate::ui::{self, Theme};
use crate::util::{format_clock, format_duration, now_secs, single_line, truncate};

const PANEL_SELECT: &str = "ticket_category";
const BUG_PRODUCT: &str = "ticket_bug_product";
const BUG_VERSION: &str = "ticket_bug_version|";
const BUG_OS: &str = "ticket_bug_os|";
const BACK_PRODUCT: &str = "ticket_back_product";
const BACK_VERSION: &str = "ticket_back_version|";
const OPEN_MODAL: &str = "ticket_open|";
const CLAIM_ID: &str = "ticket_claim";
const HOLD_ID: &str = "ticket_hold";
const CLOSE_ID: &str = "ticket_close";
const CLOSE_MODAL: &str = "ticket_close_modal";
const DELETE_DELAY: u64 = 10;

pub struct TicketCategory {
    pub key: &'static str,
    pub label: &'static str,
    pub description: &'static str,
}

pub const CATEGORIES: &[TicketCategory] = &[
    TicketCategory {
        key: "bug",
        label: "Report a bug",
        description: "Something is broken in one of our products",
    },
    TicketCategory {
        key: "license",
        label: "Licence key",
        description: "Activation, transfer or hardware change",
    },
    TicketCategory {
        key: "feature",
        label: "Suggestion",
        description: "Share an idea for a future release",
    },
    TicketCategory {
        key: "faq",
        label: "General question",
        description: "Anything else you need a hand with",
    },
];

struct Product {
    key: &'static str,
    label: &'static str,
}

const PRODUCTS: &[Product] = &[
    Product {
        key: "badomen",
        label: "BadOmen Visuals",
    },
    Product {
        key: "nouga",
        label: "Nouga Launcher",
    },
    Product {
        key: "norvoro",
        label: "Norvoro Server",
    },
];

fn category_of(key: &str) -> Option<&'static TicketCategory> {
    CATEGORIES.iter().find(|category| category.key == key)
}

fn product_label(key: &str) -> &'static str {
    PRODUCTS
        .iter()
        .find(|product| product.key == key)
        .map(|product| product.label)
        .unwrap_or("Unknown product")
}

fn versions_for(product_key: &str) -> Vec<(&'static str, &'static str)> {
    match product_key {
        "badomen" => vec![
            ("visual_2_0_0", "BadOmen Visuals 2.0.0"),
            ("visual_1_0_0", "BadOmen Visual 1.0.0"),
            ("visual_alpha", "BadOmen Visual (alpha)"),
            ("donut_1_0_0", "BadOmen Donut 1.0.0 (unmaintained)"),
        ],
        "nouga" => vec![("nouga_1_0_0", "Nouga Launcher 1.0.0")],
        "norvoro" => vec![("norvoro_0_1_0", "Norvoro Server 0.1.0")],
        _ => vec![],
    }
}

fn version_label(product_key: &str, version_key: &str) -> &'static str {
    versions_for(product_key)
        .into_iter()
        .find(|(key, _)| *key == version_key)
        .map(|(_, label)| label)
        .unwrap_or("Unknown version")
}

fn os_options(product_key: &str) -> Vec<SelectOption> {
    match product_key {
        "norvoro" => vec![
            SelectOption::new("Linux server (Ubuntu or Debian)", "linux_debian"),
            SelectOption::new("Windows Server", "windows_server"),
            SelectOption::new("Docker container", "docker"),
        ],
        _ => vec![
            SelectOption::new("Windows 11", "windows_11"),
            SelectOption::new("Windows 10", "windows_10"),
            SelectOption::new("macOS", "macos"),
            SelectOption::new("Linux", "linux"),
        ],
    }
}

fn os_label(key: &str) -> &'static str {
    match key {
        "windows_11" => "Windows 11",
        "windows_10" => "Windows 10",
        "macos" => "macOS",
        "linux" => "Linux",
        "linux_debian" => "Linux server (Ubuntu or Debian)",
        "windows_server" => "Windows Server",
        "docker" => "Docker container",
        _ => "Unknown system",
    }
}

#[derive(Clone)]
pub struct TicketService {
    pub config: Arc<ConfigStore>,
    pub tickets: Arc<TicketStore>,
    pub logger: Arc<Logger>,
    pub scheduler: Arc<Scheduler>,
    pub rest: Arc<RestClient>,
}

impl TicketService {
    fn theme(&self, guild_id: &str) -> Theme {
        Theme::from_brand(&self.config.brand(guild_id))
    }
}

pub struct TicketCommand {
    pub service: TicketService,
}

impl SlashCommand for TicketCommand {
    fn definition(&self) -> CommandDefinition {
        CommandDefinition::new("ticket", "Ticket system")
            .guild_only()
            .option(
                CommandOption::subcommand("panel", "Publish the ticket opening panel").option(
                    CommandOption::channel(
                        "channel",
                        "Target channel, defaults to the current one",
                    )
                    .channel_types(vec![CHANNEL_TYPE_GUILD_TEXT]),
                ),
            )
            .option(
                CommandOption::subcommand("close", "Close the current ticket")
                    .option(CommandOption::string("reason", "Why is it closed")),
            )
            .option(
                CommandOption::subcommand("add", "Give someone access to this ticket")
                    .option(CommandOption::user("member", "Member to add").required(true)),
            )
            .option(
                CommandOption::subcommand("remove", "Revoke access to this ticket")
                    .option(CommandOption::user("member", "Member to remove").required(true)),
            )
    }

    fn execute(&self, ctx: &CommandContext) -> Result<()> {
        let Some(guild_id) = ctx.guild_id() else {
            return ctx.reply_widget_hidden(ui::fail(
                "Server only",
                "This command can only be used inside a server.",
            ));
        };

        match ctx.subcommand() {
            Some("panel") => self.panel(ctx, guild_id),
            Some("close") => close_flow(
                &self.service,
                ctx,
                guild_id,
                ctx.option_string("reason").map(String::from),
            ),
            Some("add") => self.access(ctx, guild_id, true),
            Some("remove") => self.access(ctx, guild_id, false),
            _ => ctx.reply_widget_hidden(ui::fail(
                "Unknown action",
                "This subcommand does not exist.",
            )),
        }
    }
}

impl TicketCommand {
    fn panel(&self, ctx: &CommandContext, guild_id: &str) -> Result<()> {
        if !ctx.has_permission(PERM_MANAGE_GUILD) {
            return ctx.reply_widget_hidden(ui::fail(
                "Not allowed",
                "Publishing the panel requires the Manage Server permission.",
            ));
        }

        let target = ctx
            .option_channel("channel")
            .map(|channel| channel.id.clone())
            .or_else(|| ctx.channel_id().map(String::from));
        let Some(channel_id) = target else {
            return ctx.reply_widget_hidden(ui::fail("No channel", "No channel context."));
        };

        let cfg = self.service.config.get(guild_id);
        let theme = self.service.theme(guild_id);

        ctx.send(
            &channel_id,
            MessagePayload::widget(panel_widget(&theme, &cfg)).no_mentions(),
        )?;

        ctx.reply_widget_hidden(ui::ok(
            "Panel published",
            format!(
                "Members can now open a ticket from {}.",
                ui::channel(&channel_id)
            ),
        ))
    }

    fn access(&self, ctx: &CommandContext, guild_id: &str, grant: bool) -> Result<()> {
        let Some(channel_id) = ctx.channel_id() else {
            return ctx.reply_widget_hidden(ui::fail("Missing channel", "No channel context."));
        };
        let Some(record) = self.service.tickets.get(channel_id) else {
            return ctx.reply_widget_hidden(ui::fail(
                "Not a ticket",
                "This command only works inside a ticket channel.",
            ));
        };
        let cfg = self.service.config.get(guild_id);
        if !cfg.is_staff(ctx.member_roles()) && !ctx.is_admin() {
            return ctx.reply_widget_hidden(ui::fail(
                "Not allowed",
                "Only the staff can change who has access to a ticket.",
            ));
        }
        let Some(member) = ctx.option_user("member") else {
            return ctx.reply_widget_hidden(ui::fail("Not found", "Member not resolved."));
        };

        if grant {
            ctx.set_channel_permission(
                channel_id,
                &PermissionOverwrite::allow_member(&member.id, TICKET_MEMBER_PERMS),
            )?;
            self.service.tickets.update(channel_id, |r| {
                if !r.invited.contains(&member.id) {
                    r.invited.push(member.id.clone());
                }
            });
        } else {
            if member.id == record.opener_id {
                return ctx.reply_widget_hidden(ui::fail(
                    "Not allowed",
                    "The ticket author cannot be removed from their own ticket.",
                ));
            }
            ctx.clear_channel_permission(channel_id, &member.id)?;
            self.service.tickets.update(channel_id, |r| {
                r.invited.retain(|id| id != &member.id);
            });
        }

        self.service.logger.audit(
            guild_id,
            AuditEntry::new(
                LOG_TICKETS,
                if grant {
                    "Ticket access granted"
                } else {
                    "Ticket access revoked"
                },
            )
            .accent(ui::NEUTRAL)
            .actor(ctx.author().map(|a| a.id.as_str()).unwrap_or_default())
            .target(ui::channel(channel_id))
            .field("Member", member.mention()),
        );

        ctx.reply_widget_hidden(ui::ok(
            if grant {
                "Access granted"
            } else {
                "Access revoked"
            },
            format!(
                "{} {} this ticket.",
                member.mention(),
                if grant {
                    "can now see"
                } else {
                    "no longer sees"
                }
            ),
        ))
    }
}

fn panel_widget(theme: &Theme, cfg: &GuildConfig) -> Vec<Component> {
    let mut body = Vec::new();

    if let Some(banner) = &theme.banner {
        body.push(ui::banner(banner.clone()));
    }

    body.push(ui::header(
        theme,
        &format!("{} · Support", theme.name),
        Some("Pick the topic that matches your situation, we take it from there."),
    ));
    body.push(ui::divider());

    body.push(ui::section_thumb(
        vec![],
        "https://raw.githubusercontent.com/Mewax07/Discord.rs/main/assets/main/ticket_banner.gif",
    ));
    body.push(ui::divider());

    body.push(ui::text(ui::bullets(&[
        "One ticket per issue keeps the history readable".to_string(),
        "Describe what happens, what you expected and how to reproduce it".to_string(),
        "Screenshots, logs and versions speed everything up".to_string(),
        "No direct pings, a team member picks the ticket up".to_string(),
    ])));
    body.push(ui::divider());

    body.push(ui::text(
        CATEGORIES
            .iter()
            .map(|category| {
                let ping = cfg
                    .category_roles
                    .get(category.key)
                    .map(|id| format!(" · handled by {}", ui::role(id)))
                    .unwrap_or_default();
                format!(
                    "**{}**{ping}\n{}",
                    category.label,
                    ui::note(category.description)
                )
            })
            .collect::<Vec<_>>()
            .join("\n\n"),
    ));

    let options = CATEGORIES
        .iter()
        .map(|category| {
            SelectOption::new(category.label, category.key).description(category.description)
        })
        .collect();

    body.push(ui::menu(
        SelectMenu::new(PANEL_SELECT, options).placeholder("Open a ticket"),
    ));
    body.push(ui::text(ui::note(&theme.footer)));

    ui::panel(theme.accent, body)
}

fn wizard(
    step: u8,
    heading: &str,
    detail: &str,
    theme: &Theme,
    rows: Vec<Component>,
) -> Vec<Component> {
    let mut body = vec![ui::lines(vec![
        ui::subtitle(heading),
        ui::note(format!("Step {step} of 4 · {detail}")),
    ])];
    body.push(ui::divider());
    body.extend(rows);
    ui::panel(theme.accent, body)
}

fn product_step(ctx: &CommandContext, theme: &Theme) -> Vec<Component> {
    let _ = ctx;
    let options = PRODUCTS
        .iter()
        .map(|product| SelectOption::new(product.label, product.key))
        .collect();

    wizard(
        1,
        "Report a bug",
        "Which product is affected",
        theme,
        vec![ui::menu(
            SelectMenu::new(BUG_PRODUCT, options).placeholder("Select a product"),
        )],
    )
}

fn version_step(product_key: &str, theme: &Theme) -> Vec<Component> {
    let options = versions_for(product_key)
        .into_iter()
        .map(|(key, label)| SelectOption::new(label, key))
        .collect();

    wizard(
        2,
        "Report a bug",
        &format!("Version of {}", product_label(product_key)),
        theme,
        vec![
            ui::menu(
                SelectMenu::new(format!("{BUG_VERSION}{product_key}"), options)
                    .placeholder("Select a version"),
            ),
            ui::row(vec![Button::new(
                "Back",
                BACK_PRODUCT,
                ButtonStyle::Secondary,
            )]),
        ],
    )
}

fn os_step(product_key: &str, version_key: &str, theme: &Theme) -> Vec<Component> {
    wizard(
        3,
        "Report a bug",
        &format!(
            "{} · {}",
            product_label(product_key),
            version_label(product_key, version_key)
        ),
        theme,
        vec![
            ui::menu(
                SelectMenu::new(
                    format!("{BUG_OS}{product_key}|{version_key}"),
                    os_options(product_key),
                )
                .placeholder("Select your system"),
            ),
            ui::row(vec![Button::new(
                "Back",
                format!("{BACK_VERSION}{product_key}"),
                ButtonStyle::Secondary,
            )]),
        ],
    )
}

fn modal_fields(category_key: &str) -> Vec<ActionRow> {
    match category_key {
        "bug" => vec![
            ActionRow::input(
                TextInput::new("summary", "One line summary", TextInputStyle::Short)
                    .placeholder("Shaders flicker when opening the menu")
                    .max_length(100),
            ),
            ActionRow::input(
                TextInput::new("steps", "Steps to reproduce", TextInputStyle::Paragraph)
                    .placeholder("1. Launch the client\n2. Open the settings\n3. ...")
                    .max_length(1000),
            ),
            ActionRow::input(
                TextInput::new("specs", "Hardware", TextInputStyle::Short)
                    .required(false)
                    .placeholder("GPU, CPU, RAM")
                    .max_length(200),
            ),
            ActionRow::input(
                TextInput::new("logs", "Logs or error message", TextInputStyle::Paragraph)
                    .required(false)
                    .placeholder("Paste the error or a link to the full log")
                    .max_length(1000),
            ),
        ],
        "license" => vec![
            ActionRow::input(
                TextInput::new("summary", "Account name", TextInputStyle::Short).max_length(100),
            ),
            ActionRow::input(
                TextInput::new("details", "What do you need", TextInputStyle::Paragraph)
                    .placeholder("Activation, hardware change, transfer, invoice")
                    .max_length(1000),
            ),
            ActionRow::input(
                TextInput::new("hwid", "Hardware identifier", TextInputStyle::Short)
                    .required(false)
                    .max_length(120),
            ),
        ],
        "feature" => vec![
            ActionRow::input(
                TextInput::new("summary", "Idea in one line", TextInputStyle::Short)
                    .max_length(100),
            ),
            ActionRow::input(
                TextInput::new("details", "How should it work", TextInputStyle::Paragraph)
                    .placeholder("What problem it solves, how you imagine it")
                    .max_length(1000),
            ),
        ],
        _ => vec![
            ActionRow::input(
                TextInput::new("summary", "Question in one line", TextInputStyle::Short)
                    .max_length(100),
            ),
            ActionRow::input(
                TextInput::new("details", "Give us the context", TextInputStyle::Paragraph)
                    .max_length(1000),
            ),
        ],
    }
}

fn collect_context(
    ctx: &CommandContext,
    category_key: &str,
    parts: &[&str],
) -> Vec<(String, String)> {
    let mut context = Vec::new();

    if category_key == "bug" {
        if let (Some(product), Some(version), Some(os)) = (parts.get(1), parts.get(2), parts.get(3))
        {
            context.push(("Product".to_string(), product_label(product).to_string()));
            context.push((
                "Version".to_string(),
                version_label(product, version).to_string(),
            ));
            context.push(("System".to_string(), os_label(os).to_string()));
        }
    }

    for (field, label) in [
        ("steps", "Steps to reproduce"),
        ("details", "Details"),
        ("specs", "Hardware"),
        ("hwid", "Hardware identifier"),
        ("logs", "Logs"),
    ] {
        if let Some(value) = ctx.modal_text(field) {
            context.push((label.to_string(), truncate(value, 900)));
        }
    }

    context
}

fn ticket_widget(record: &TicketRecord, theme: &Theme, cfg: &GuildConfig) -> Vec<Component> {
    let label = category_of(&record.category)
        .map(|category| category.label)
        .unwrap_or("Ticket");

    let mut head = vec![
        ui::title(format!("Ticket #{:04} · {label}", record.number)),
        ui::note(format!(
            "Opened by {} · {}",
            ui::user(&record.opener_id),
            ui::relative(record.opened_at)
        )),
    ];
    if let Some(subject) = &record.subject {
        head.push(format!("**{subject}**"));
    }

    let mut facts = vec![
        ui::kv("Status", record.status_label()),
        ui::kv(
            "Assigned",
            record
                .claimed_by
                .as_ref()
                .map(|id| ui::user(id))
                .unwrap_or_else(|| "waiting for a team member".to_string()),
        ),
    ];
    for (name, value) in record.context.iter().take(3) {
        if value.len() <= 60 {
            facts.push(ui::kv(name, value));
        }
    }

    let mut body = vec![
        match &theme.logo {
            Some(logo) => ui::section_thumb(head, logo.clone()),
            None => ui::lines(head),
        },
        ui::divider(),
        ui::lines(facts),
    ];

    let details: Vec<String> = record
        .context
        .iter()
        .filter(|(_, value)| value.len() > 60)
        .map(|(name, value)| ui::entry(name, value))
        .collect();

    if !details.is_empty() {
        body.push(ui::divider());
        body.push(ui::text(details.join("\n\n")));
    }

    body.push(ui::divider());
    body.push(ui::row(vec![
        Button::new(
            if record.claimed_by.is_some() {
                "Reassign to me"
            } else {
                "Claim"
            },
            CLAIM_ID,
            ButtonStyle::Primary,
        ),
        Button::new(
            if record.on_hold {
                "Resume"
            } else {
                "Put on hold"
            },
            HOLD_ID,
            ButtonStyle::Secondary,
        ),
        Button::new("Close", CLOSE_ID, ButtonStyle::Danger),
    ]));

    let ping = cfg
        .category_roles
        .get(&record.category)
        .map(|id| format!(" · {}", ui::role(id)))
        .unwrap_or_default();
    body.push(ui::text(ui::note(format!(
        "{} · {}{ping}",
        theme.footer,
        ui::user(&record.opener_id)
    ))));

    ui::panel(
        if record.on_hold {
            ui::WARNING
        } else {
            theme.accent
        },
        body,
    )
}

pub struct TicketPanelHandler {
    pub service: TicketService,
}

impl ComponentHandler for TicketPanelHandler {
    fn matches(&self, custom_id: &str) -> bool {
        custom_id == PANEL_SELECT
    }

    fn execute(&self, ctx: &CommandContext) -> Result<()> {
        let Some(guild_id) = ctx.guild_id() else {
            return ctx
                .reply_widget_hidden(ui::fail("Server only", "Use the panel inside the server."));
        };
        let Some(category) = ctx.selected_value().and_then(category_of) else {
            return ctx
                .reply_widget_hidden(ui::fail("Unknown topic", "This category does not exist."));
        };

        let open = self
            .service
            .tickets
            .open_for(guild_id, ctx.author().map(|a| a.id.as_str()).unwrap_or(""));
        if open.len() >= 3 {
            return ctx.reply_widget_hidden(ui::warn(
                "Too many open tickets",
                "Close one of your current tickets before opening another one.",
            ));
        }

        if category.key == "bug" {
            let theme = self.service.theme(guild_id);
            return ctx.reply(
                MessagePayload::widget(product_step(ctx, &theme))
                    .ephemeral()
                    .no_mentions(),
            );
        }

        ctx.show_modal(
            format!("{OPEN_MODAL}{}", category.key),
            category.label,
            modal_fields(category.key),
        )
    }
}

pub struct TicketBugProductHandler {
    pub service: TicketService,
}

impl ComponentHandler for TicketBugProductHandler {
    fn matches(&self, custom_id: &str) -> bool {
        custom_id == BUG_PRODUCT
    }

    fn execute(&self, ctx: &CommandContext) -> Result<()> {
        let Some(product) = ctx.selected_value() else {
            return ctx.reply_widget_hidden(ui::fail("Unknown product", "Invalid selection."));
        };
        let theme = self.service.theme(ctx.guild_id().unwrap_or_default());
        ctx.update(MessagePayload::widget(version_step(product, &theme)).ephemeral())
    }
}

pub struct TicketBugBackToProductHandler {
    pub service: TicketService,
}

impl ComponentHandler for TicketBugBackToProductHandler {
    fn matches(&self, custom_id: &str) -> bool {
        custom_id == BACK_PRODUCT
    }

    fn execute(&self, ctx: &CommandContext) -> Result<()> {
        let theme = self.service.theme(ctx.guild_id().unwrap_or_default());
        ctx.update(MessagePayload::widget(product_step(ctx, &theme)).ephemeral())
    }
}

pub struct TicketBugVersionHandler {
    pub service: TicketService,
}

impl ComponentHandler for TicketBugVersionHandler {
    fn matches(&self, custom_id: &str) -> bool {
        custom_id.starts_with(BUG_VERSION)
    }

    fn execute(&self, ctx: &CommandContext) -> Result<()> {
        let Some(custom_id) = ctx.custom_id() else {
            return ctx.reply_widget_hidden(ui::fail("Missing context", "Invalid selection."));
        };
        let product = custom_id.trim_start_matches(BUG_VERSION);
        let Some(version) = ctx.selected_value() else {
            return ctx.reply_widget_hidden(ui::fail("Unknown version", "Invalid selection."));
        };
        let theme = self.service.theme(ctx.guild_id().unwrap_or_default());
        ctx.update(MessagePayload::widget(os_step(product, version, &theme)).ephemeral())
    }
}

pub struct TicketBugBackToVersionHandler {
    pub service: TicketService,
}

impl ComponentHandler for TicketBugBackToVersionHandler {
    fn matches(&self, custom_id: &str) -> bool {
        custom_id.starts_with(BACK_VERSION)
    }

    fn execute(&self, ctx: &CommandContext) -> Result<()> {
        let Some(custom_id) = ctx.custom_id() else {
            return ctx.reply_widget_hidden(ui::fail("Missing context", "Invalid selection."));
        };
        let product = custom_id.trim_start_matches(BACK_VERSION);
        let theme = self.service.theme(ctx.guild_id().unwrap_or_default());
        ctx.update(MessagePayload::widget(version_step(product, &theme)).ephemeral())
    }
}

pub struct TicketBugOsHandler;

impl ComponentHandler for TicketBugOsHandler {
    fn matches(&self, custom_id: &str) -> bool {
        custom_id.starts_with(BUG_OS)
    }

    fn execute(&self, ctx: &CommandContext) -> Result<()> {
        let Some(custom_id) = ctx.custom_id() else {
            return ctx.reply_widget_hidden(ui::fail("Missing context", "Invalid selection."));
        };
        let mut parts = custom_id.trim_start_matches(BUG_OS).splitn(2, '|');
        let (Some(product), Some(version), Some(os)) =
            (parts.next(), parts.next(), ctx.selected_value())
        else {
            return ctx.reply_widget_hidden(ui::fail("Missing context", "Invalid selection."));
        };

        ctx.show_modal(
            format!("{OPEN_MODAL}bug|{product}|{version}|{os}"),
            "Report a bug",
            modal_fields("bug"),
        )
    }
}

pub struct TicketOpenHandler {
    pub service: TicketService,
}

impl ComponentHandler for TicketOpenHandler {
    fn matches(&self, custom_id: &str) -> bool {
        custom_id.starts_with(OPEN_MODAL)
    }

    fn execute(&self, ctx: &CommandContext) -> Result<()> {
        let Some(guild_id) = ctx.guild_id() else {
            return ctx
                .reply_widget_hidden(ui::fail("Server only", "Open your ticket from the server."));
        };
        let Some(author) = ctx.author() else {
            return ctx.reply_widget_hidden(ui::fail("Unknown user", "Unable to identify you."));
        };
        let Some(custom_id) = ctx.custom_id() else {
            return ctx.reply_widget_hidden(ui::fail("Missing context", "Invalid form."));
        };

        let key = custom_id.trim_start_matches(OPEN_MODAL);
        let parts: Vec<&str> = key.split('|').collect();
        let Some(category) = parts.first().and_then(|k| category_of(k)) else {
            return ctx
                .reply_widget_hidden(ui::fail("Unknown topic", "This category does not exist."));
        };

        let cfg = self.service.config.get(guild_id);
        let theme = self.service.theme(guild_id);
        let number = self.service.config.next_ticket_number(guild_id);

        let mut overwrites = vec![
            PermissionOverwrite::deny_everyone(guild_id, PERM_VIEW_CHANNEL),
            PermissionOverwrite::allow_member(&author.id, TICKET_MEMBER_PERMS),
        ];
        if let Some(staff_role) = &cfg.staff_role_id {
            overwrites.push(PermissionOverwrite::allow_role(
                staff_role,
                TICKET_MEMBER_PERMS,
            ));
        }
        if let Some(role_id) = cfg.category_roles.get(category.key) {
            if cfg.staff_role_id.as_deref() != Some(role_id.as_str()) {
                overwrites.push(PermissionOverwrite::allow_role(
                    role_id,
                    TICKET_MEMBER_PERMS,
                ));
            }
        }

        let subject = ctx.modal_text("summary").map(|value| truncate(value, 100));
        let channel = NewChannel::text(format!("{}-{number:04}", category.key))
            .parent(cfg.ticket_category_id.as_deref())
            .topic(format!(
                "Ticket #{number:04} · {} · opened by {} ({})",
                category.label,
                author.display_name(),
                author.id
            ))
            .overwrites(overwrites);

        let channel = match ctx.create_channel(guild_id, channel) {
            Ok(channel) => channel,
            Err(e) => {
                logs::error("ticket", format!("channel creation failed: {e}"));
                return ctx.reply_widget_hidden(ui::fail(
                    "Ticket not created",
                    format!(
                        "The ticket category looks invalid. Ask an admin to run {}.",
                        ui::code("/config tickets category")
                    ),
                ));
            }
        };

        let record = TicketRecord {
            guild_id: guild_id.to_string(),
            channel_id: channel.id.clone(),
            opener_id: author.id.clone(),
            category: category.key.to_string(),
            opened_at: now_secs(),
            claimed_by: None,
            on_hold: false,
            number,
            subject: subject.clone(),
            context: collect_context(ctx, category.key, &parts),
            invited: Vec::new(),
        };
        self.service.tickets.insert(&channel.id, record.clone());

        let mut mention_roles = Vec::new();
        if let Some(role_id) = cfg.category_roles.get(category.key) {
            mention_roles.push(role_id.clone());
        }

        ctx.reply(
            MessagePayload::widget(vec![ui::section_button(
                vec![
                    ui::subtitle("Ticket created"),
                    format!(
                        "Everything happens in {} from now on.",
                        ui::channel(&channel.id)
                    ),
                    ui::note("A team member picks it up as soon as one is available."),
                ],
                Button::link(
                    "Open ticket",
                    format!("https://discord.com/channels/{guild_id}/{}", channel.id),
                ),
            )])
            .ephemeral(),
        )?;

        ctx.send(
            &channel.id,
            MessagePayload::widget(ticket_widget(&record, &theme, &cfg)).mentions(
                AllowedMentions::users_and_roles(vec![author.id.clone()], mention_roles),
            ),
        )?;

        self.service.logger.audit(
            guild_id,
            AuditEntry::new(LOG_TICKETS, "Ticket opened")
                .accent(theme.accent)
                .actor(&author.id)
                .target(ui::channel(&channel.id))
                .field("Number", format!("#{number:04}"))
                .field("Topic", category.label.to_string())
                .maybe_field("Summary", subject),
        );
        logs::info("ticket", format!("#{number:04} opened by {}", author.id));

        Ok(())
    }
}

fn staff_guard(ctx: &CommandContext, cfg: &GuildConfig) -> bool {
    cfg.is_staff(ctx.member_roles()) || ctx.is_admin()
}

pub struct TicketClaimHandler {
    pub service: TicketService,
}

impl ComponentHandler for TicketClaimHandler {
    fn matches(&self, custom_id: &str) -> bool {
        custom_id == CLAIM_ID
    }

    fn execute(&self, ctx: &CommandContext) -> Result<()> {
        let (Some(guild_id), Some(channel_id), Some(author)) =
            (ctx.guild_id(), ctx.channel_id(), ctx.author())
        else {
            return ctx.reply_widget_hidden(ui::fail("Missing context", "No ticket context."));
        };
        let cfg = self.service.config.get(guild_id);
        if !staff_guard(ctx, &cfg) {
            return ctx.reply_widget_hidden(ui::fail(
                "Not allowed",
                "Only the staff can claim a ticket.",
            ));
        }

        let Some(record) = self.service.tickets.update(channel_id, |r| {
            r.claimed_by = Some(author.id.clone());
            r.on_hold = false;
        }) else {
            return ctx.reply_widget_hidden(ui::fail(
                "Not a ticket",
                "This channel is not tracked as a ticket.",
            ));
        };

        self.service.logger.audit(
            guild_id,
            AuditEntry::new(LOG_TICKETS, "Ticket claimed")
                .accent(ui::ACCENT)
                .actor(&author.id)
                .target(ui::channel(channel_id))
                .field("Number", format!("#{:04}", record.number)),
        );

        let theme = self.service.theme(guild_id);
        ctx.update(MessagePayload::widget(ticket_widget(&record, &theme, &cfg)).no_mentions())
    }
}

pub struct TicketHoldHandler {
    pub service: TicketService,
}

impl ComponentHandler for TicketHoldHandler {
    fn matches(&self, custom_id: &str) -> bool {
        custom_id == HOLD_ID
    }

    fn execute(&self, ctx: &CommandContext) -> Result<()> {
        let (Some(guild_id), Some(channel_id), Some(author)) =
            (ctx.guild_id(), ctx.channel_id(), ctx.author())
        else {
            return ctx.reply_widget_hidden(ui::fail("Missing context", "No ticket context."));
        };
        let cfg = self.service.config.get(guild_id);
        if !staff_guard(ctx, &cfg) {
            return ctx.reply_widget_hidden(ui::fail(
                "Not allowed",
                "Only the staff can change the ticket status.",
            ));
        }

        let Some(record) = self
            .service
            .tickets
            .update(channel_id, |r| r.on_hold = !r.on_hold)
        else {
            return ctx.reply_widget_hidden(ui::fail(
                "Not a ticket",
                "This channel is not tracked as a ticket.",
            ));
        };

        self.service.logger.audit(
            guild_id,
            AuditEntry::new(
                LOG_TICKETS,
                if record.on_hold {
                    "Ticket put on hold"
                } else {
                    "Ticket resumed"
                },
            )
            .accent(ui::WARNING)
            .actor(&author.id)
            .target(ui::channel(channel_id))
            .field("Number", format!("#{:04}", record.number)),
        );

        let theme = self.service.theme(guild_id);
        ctx.update(MessagePayload::widget(ticket_widget(&record, &theme, &cfg)).no_mentions())
    }
}

pub struct TicketCloseHandler {
    pub service: TicketService,
}

impl ComponentHandler for TicketCloseHandler {
    fn matches(&self, custom_id: &str) -> bool {
        custom_id == CLOSE_ID
    }

    fn execute(&self, ctx: &CommandContext) -> Result<()> {
        let Some(channel_id) = ctx.channel_id() else {
            return ctx.reply_widget_hidden(ui::fail("Missing context", "No ticket context."));
        };
        if self.service.tickets.get(channel_id).is_none() {
            return ctx.reply_widget_hidden(ui::fail(
                "Not a ticket",
                "This channel is not tracked as a ticket.",
            ));
        }

        ctx.show_modal(
            CLOSE_MODAL,
            "Close this ticket",
            vec![ActionRow::input(
                TextInput::new("reason", "Resolution", TextInputStyle::Paragraph)
                    .required(false)
                    .placeholder("What was the outcome? Visible in the logs and the recap.")
                    .max_length(500),
            )],
        )
    }
}

pub struct TicketCloseModalHandler {
    pub service: TicketService,
}

impl ComponentHandler for TicketCloseModalHandler {
    fn matches(&self, custom_id: &str) -> bool {
        custom_id == CLOSE_MODAL
    }

    fn execute(&self, ctx: &CommandContext) -> Result<()> {
        let Some(guild_id) = ctx.guild_id() else {
            return ctx.reply_widget_hidden(ui::fail("Server only", "No server context."));
        };
        let reason = ctx.modal_text("reason").map(String::from);
        close_flow(&self.service, ctx, guild_id, reason)
    }
}

fn close_flow(
    service: &TicketService,
    ctx: &CommandContext,
    guild_id: &str,
    reason: Option<String>,
) -> Result<()> {
    let Some(channel_id) = ctx.channel_id() else {
        return ctx.reply_widget_hidden(ui::fail("Missing context", "No ticket context."));
    };
    let Some(closer) = ctx.author() else {
        return ctx.reply_widget_hidden(ui::fail("Unknown user", "Unable to identify you."));
    };
    let Some(record) = service.tickets.get(channel_id) else {
        return ctx.reply_widget_hidden(ui::fail(
            "Not a ticket",
            "This channel is not tracked as a ticket.",
        ));
    };

    let cfg = service.config.get(guild_id);
    if !staff_guard(ctx, &cfg) && closer.id != record.opener_id {
        return ctx.reply_widget_hidden(ui::fail(
            "Not allowed",
            "Only the staff or the ticket author can close this ticket.",
        ));
    }

    let theme = service.theme(guild_id);
    ctx.reply_widget_hidden(ui::ok(
        "Ticket closing",
        format!("The archive is being written, the channel disappears in {DELETE_DELAY} seconds."),
    ))?;

    let duration = now_secs().saturating_sub(record.opened_at);
    let messages = ctx.fetch_all_messages(channel_id).unwrap_or_default();
    let mut participants = HashSet::new();
    let mut transcript = String::new();

    transcript.push_str(&format!(
        "Ticket #{:04} · {}\nOpened by {} ({}) on {}\nClosed by {} ({}) on {}\nDuration {}\n",
        record.number,
        category_of(&record.category)
            .map(|c| c.label)
            .unwrap_or("Ticket"),
        record.opener_id,
        record.opener_id,
        format_clock(record.opened_at),
        closer.display_name(),
        closer.id,
        format_clock(now_secs()),
        format_duration(duration)
    ));
    if let Some(reason) = &reason {
        transcript.push_str(&format!("Resolution: {}\n", single_line(reason)));
    }
    transcript.push_str(&"-".repeat(72));
    transcript.push('\n');

    for message in &messages {
        participants.insert(message.author.id.clone());
        let stamp = message.timestamp.chars().take(19).collect::<String>();
        transcript.push_str(&format!(
            "[{stamp}] {}: {}\n",
            message.author.display_name(),
            message.content
        ));
        for attachment in &message.attachments {
            transcript.push_str(&format!("    attachment · {}\n", attachment.url));
        }
    }

    let entry = AuditEntry::new(LOG_TICKETS, "Ticket closed")
        .accent(ui::DANGER)
        .actor(&closer.id)
        .target(ui::channel(channel_id))
        .field("Number", format!("#{:04}", record.number))
        .field(
            "Topic",
            category_of(&record.category)
                .map(|c| c.label.to_string())
                .unwrap_or_else(|| record.category.clone()),
        )
        .field("Author", ui::user(&record.opener_id))
        .maybe_field(
            "Handled by",
            record.claimed_by.as_ref().map(|id| ui::user(id)),
        )
        .field("Duration", format_duration(duration))
        .field("Messages", messages.len().to_string())
        .field("Participants", participants.len().to_string())
        .maybe_field("Resolution", reason.clone());

    if cfg.ticket_transcripts {
        let file_name = format!("ticket-{:04}.txt", record.number);
        service
            .logger
            .audit_with_file(guild_id, entry, &file_name, transcript.as_bytes());
    } else {
        service.logger.audit(guild_id, entry);
    }

    if cfg.ticket_dm_summary {
        let recap = ui::panel(
            theme.accent,
            vec![ui::lines(vec![
                ui::subtitle(format!("Ticket #{:04} closed", record.number)),
                ui::kv(
                    "Topic",
                    category_of(&record.category)
                        .map(|c| c.label)
                        .unwrap_or("Ticket"),
                ),
                ui::kv("Open for", format_duration(duration)),
                ui::kv("Closed by", closer.display_name()),
                reason
                    .clone()
                    .map(|value| ui::entry("Resolution", value))
                    .unwrap_or_else(|| ui::note("No resolution note was written.")),
                ui::note(&theme.footer),
            ])],
        );
        if let Err(e) = ctx.direct_message(
            &record.opener_id,
            MessagePayload::widget(recap).no_mentions(),
        ) {
            logs::debug("ticket", format!("recap not delivered: {e}"));
        }
    }

    let goodbye = ui::panel(
        ui::DANGER,
        vec![ui::lines(vec![
            ui::subtitle("Ticket closed"),
            format!(
                "Closed by {} · the channel is removed {}.",
                ui::user(&closer.id),
                ui::relative(now_secs() + DELETE_DELAY)
            ),
            reason
                .map(|value| ui::entry("Resolution", value))
                .unwrap_or_default(),
        ])],
    );
    let _ = ctx.send(channel_id, MessagePayload::widget(goodbye).no_mentions());

    service.tickets.remove(channel_id);
    logs::info(
        "ticket",
        format!("#{:04} closed by {}", record.number, closer.id),
    );

    let rest = service.rest.clone();
    let target = channel_id.to_string();
    service
        .scheduler
        .schedule_at(now_secs() + DELETE_DELAY, move || {
            if let Err(e) = rest.delete_channel(&target) {
                logs::error("ticket", format!("channel deletion failed: {e}"));
            }
        });

    Ok(())
}
