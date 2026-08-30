use std::sync::Arc;

use discord::commands::{CommandContext, ComponentHandler, SlashCommand};
use discord::error::Result;
use discord::models::{
    AllowedMentions, Button, ButtonStyle, CommandDefinition, CommandOption, Component,
    MessagePayload, PERM_ADMINISTRATOR,
};
use discord::rest::RestClient;
use licensing::{IssueRequest, LicenseService};

use crate::commands::license::{may_issue, plan_days, plan_exists, PLANS};
use crate::logs::{self, AuditEntry, Logger};
use crate::scheduler::Scheduler;
use crate::storage::{ConfigStore, GiveawayRecord, GiveawayStore, LOG_GIVEAWAYS};
use crate::ui::{self, Theme};
use crate::util::{format_duration, now_secs, parse_duration, truncate};

const ENTER_ID: &str = "giveaway_enter";
const END_ID: &str = "giveaway_end";
const REROLL_ID: &str = "giveaway_reroll";
const MIN_DURATION: u64 = 60;
const MAX_DURATION: u64 = 60 * 86_400;
const REWARD_TEXT: &str = "text";
const REWARD_LICENSE: &str = "license";

#[derive(Clone)]
pub struct GiveawayService {
    pub giveaways: Arc<GiveawayStore>,
    pub config: Arc<ConfigStore>,
    pub logger: Arc<Logger>,
    pub scheduler: Arc<Scheduler>,
    pub rest: Arc<RestClient>,
    pub licenses: Arc<LicenseService>,
    pub product: String,
    pub owner_id: Option<String>,
}

impl GiveawayService {
    fn theme(&self, guild_id: &str) -> Theme {
        Theme::from_brand(&self.config.brand(guild_id))
    }
}

pub struct GiveawayCommand {
    pub service: GiveawayService,
}

impl SlashCommand for GiveawayCommand {
    fn definition(&self) -> CommandDefinition {
        let mut plan = CommandOption::string("plan", "Licence plan when the reward is a key");
        for (key, label, _) in PLANS {
            plan = plan.choice(*label, *key);
        }

        CommandDefinition::new("giveaway", "Run a giveaway")
            .guild_only()
            .required_permissions(PERM_ADMINISTRATOR)
            .option(CommandOption::string("prize", "What is being given away").required(true))
            .option(
                CommandOption::string("duration", "How long it runs, such as 45m, 6h or 3d")
                    .required(true),
            )
            .option(
                CommandOption::integer("winners", "How many winners, one by default")
                    .min_value(1)
                    .max_value(20),
            )
            .option(
                CommandOption::string("reward", "What the winners actually receive")
                    .choice("Announcement only", REWARD_TEXT)
                    .choice("Licence key sent by direct message", REWARD_LICENSE),
            )
            .option(plan)
            .option(
                CommandOption::integer("days", "Override the licence duration, in days")
                    .min_value(1)
                    .max_value(3650),
            )
            .option(CommandOption::role(
                "required_role",
                "Restrict entries to members holding this role",
            ))
    }

    fn execute(&self, ctx: &CommandContext) -> Result<()> {
        let Some(guild_id) = ctx.guild_id() else {
            return ctx.reply_widget_hidden(ui::fail(
                "Server only",
                "This command can only be used inside a server.",
            ));
        };
        if !ctx.has_permission(PERM_ADMINISTRATOR) {
            return ctx.reply_widget_hidden(ui::fail(
                "Not allowed",
                "Only administrators can start a giveaway.",
            ));
        }
        let Some(channel_id) = ctx.channel_id() else {
            return ctx.reply_widget_hidden(ui::fail("Missing channel", "No channel context."));
        };
        let Some(host) = ctx.author() else {
            return ctx.reply_widget_hidden(ui::fail("Unknown user", "Unable to identify you."));
        };
        let (Some(prize), Some(raw_duration)) =
            (ctx.option_string("prize"), ctx.option_string("duration"))
        else {
            return ctx.reply_widget_hidden(ui::fail(
                "Missing input",
                "A prize and a duration are both required.",
            ));
        };

        let Some(duration) = parse_duration(raw_duration) else {
            return ctx.reply_widget_hidden(ui::fail(
                "Invalid duration",
                format!(
                    "Use a value such as {}, {} or {}.",
                    ui::code("45m"),
                    ui::code("6h"),
                    ui::code("3d")
                ),
            ));
        };
        if !(MIN_DURATION..=MAX_DURATION).contains(&duration) {
            return ctx.reply_widget_hidden(ui::fail(
                "Invalid duration",
                "A giveaway runs for at least one minute and at most sixty days.",
            ));
        }

        let reward_kind = ctx
            .option_string("reward")
            .unwrap_or(REWARD_TEXT)
            .to_string();
        let reward_plan = ctx.option_string("plan").map(String::from);

        if reward_kind == REWARD_LICENSE {
            if !may_issue(
                ctx,
                &self.service.config,
                self.service.owner_id.as_deref(),
                guild_id,
            ) {
                return ctx.reply_widget_hidden(ui::fail(
                    "Not allowed",
                    "Only the owner and the manager role can give away licence keys. Run the giveaway with an announcement reward instead.",
                ));
            }
            match &reward_plan {
                Some(plan) if plan_exists(plan) => {}
                _ => {
                    return ctx.reply_widget_hidden(ui::fail(
                        "Missing plan",
                        "Pick a licence plan when the reward is a key.",
                    ))
                }
            }
        }

        let reward_days = ctx
            .option_integer("days")
            .map(|value| value.max(1) as u64)
            .or_else(|| reward_plan.as_deref().and_then(plan_days));

        let now = now_secs();
        let record = GiveawayRecord {
            guild_id: guild_id.to_string(),
            channel_id: channel_id.to_string(),
            message_id: String::new(),
            host_id: host.id.clone(),
            prize: truncate(prize, 200),
            winner_count: ctx.option_integer("winners").unwrap_or(1).clamp(1, 20) as u32,
            ends_at: now + duration,
            entrants: Vec::new(),
            ended: false,
            created_at: now,
            required_role_id: ctx.option_role("required_role").map(|role| role.id.clone()),
            winners: Vec::new(),
            reward_kind,
            reward_plan,
            reward_days,
            issued: Vec::new(),
        };

        let theme = self.service.theme(guild_id);
        let message = ctx.send(
            channel_id,
            MessagePayload::widget(widget(&record, &theme)).no_mentions(),
        )?;

        let mut record = record;
        record.message_id = message.id.clone();
        self.service.giveaways.insert(&message.id, record.clone());

        schedule_end(self.service.clone(), message.id.clone(), record.ends_at);

        self.service.logger.audit(
            guild_id,
            AuditEntry::new(LOG_GIVEAWAYS, "Giveaway started")
                .accent(theme.accent)
                .actor(&host.id)
                .target(ui::channel(channel_id))
                .field("Prize", record.prize.clone())
                .field("Winners", record.winner_count.to_string())
                .field("Reward", reward_label(&record))
                .field("Closes", ui::full_date(record.ends_at)),
        );
        logs::info(
            "giveaway",
            format!("started in {channel_id}: {}", record.prize),
        );

        ctx.reply(
            MessagePayload::widget(vec![ui::section_button(
                vec![
                    ui::subtitle("Giveaway published"),
                    ui::note(format!(
                        "Runs for {} · {} winner(s) · {}",
                        format_duration(duration),
                        record.winner_count,
                        reward_label(&record)
                    )),
                ],
                Button::link("Open giveaway", record.link()),
            )])
            .ephemeral(),
        )
    }
}

fn reward_label(record: &GiveawayRecord) -> String {
    if record.reward_kind != REWARD_LICENSE {
        return "announcement only".to_string();
    }

    let plan = record.reward_plan.clone().unwrap_or_default();
    match record.reward_days {
        Some(days) => format!("licence {plan} · {days} days"),
        None => format!("licence {plan} · lifetime"),
    }
}

fn widget(record: &GiveawayRecord, theme: &Theme) -> Vec<Component> {
    let mut head = vec![
        ui::title(&record.prize),
        ui::note(format!(
            "Hosted by {} · {}",
            ui::user(&record.host_id),
            if record.ended {
                format!("Ended {}", ui::relative(record.ends_at))
            } else {
                format!("Ends {}", ui::relative(record.ends_at))
            }
        )),
    ];

    if record.ended {
        head.push(if record.winners.is_empty() {
            ui::italic("Nobody entered this giveaway.")
        } else {
            format!(
                "**Winners** · {}",
                record
                    .winners
                    .iter()
                    .map(|id| ui::user(id))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        });
    }

    let mut facts = vec![
        ui::kv("Winners", record.winner_count.to_string()),
        ui::kv("Entries", record.entrants.len().to_string()),
        ui::kv("Reward", reward_label(record)),
    ];
    if let Some(role_id) = &record.required_role_id {
        facts.push(ui::kv("Requirement", ui::role(role_id)));
    }

    let mut body = vec![
        ui::lines(head),
        ui::divider(),
        ui::lines(facts),
        ui::divider(),
    ];

    if record.ended {
        body.push(ui::row(vec![Button::new(
            "Reroll",
            REROLL_ID,
            ButtonStyle::Secondary,
        )]));
    } else {
        body.push(ui::row(vec![
            Button::new("Enter", ENTER_ID, ButtonStyle::Success),
            Button::new("End now", END_ID, ButtonStyle::Danger),
        ]));
    }

    body.push(ui::text(ui::note(format!(
        "{} · Click Enter again to withdraw",
        theme.footer
    ))));

    ui::panel(
        if record.ended {
            ui::NEUTRAL
        } else {
            theme.accent
        },
        body,
    )
}

fn refresh(ctx: &CommandContext, record: &GiveawayRecord, theme: &Theme) {
    let payload = MessagePayload::widget(widget(record, theme)).no_mentions();
    if let Err(e) = ctx.edit(&record.channel_id, &record.message_id, payload) {
        logs::error("giveaway", format!("widget refresh failed: {e}"));
    }
}

pub struct GiveawayEnterHandler {
    pub service: GiveawayService,
}

impl ComponentHandler for GiveawayEnterHandler {
    fn matches(&self, custom_id: &str) -> bool {
        custom_id == ENTER_ID
    }

    fn execute(&self, ctx: &CommandContext) -> Result<()> {
        let (Some(message_id), Some(guild_id), Some(author)) =
            (ctx.message_id(), ctx.guild_id(), ctx.author())
        else {
            return ctx
                .reply_widget_hidden(ui::fail("Missing giveaway", "No giveaway attached here."));
        };
        let Some(record) = self.service.giveaways.get(message_id) else {
            return ctx.reply_widget_hidden(ui::warn(
                "Giveaway unavailable",
                "This giveaway is no longer tracked by the bot.",
            ));
        };
        if record.ended {
            return ctx.reply_widget_hidden(ui::warn(
                "Giveaway closed",
                "This giveaway has already ended.",
            ));
        }

        if let Some(role_id) = &record.required_role_id {
            if !ctx.has_role(role_id) {
                return ctx.reply_widget_hidden(ui::fail(
                    "Entry refused",
                    format!(
                        "You need {} to take part in this giveaway.",
                        ui::role(role_id)
                    ),
                ));
            }
        }

        let joined = !record.has_entered(&author.id);
        let Some(updated) = self.service.giveaways.update(message_id, |r| {
            r.toggle(&author.id);
        }) else {
            return ctx.reply_widget_hidden(ui::warn(
                "Giveaway unavailable",
                "This giveaway is no longer tracked by the bot.",
            ));
        };

        let theme = self.service.theme(guild_id);
        refresh(ctx, &updated, &theme);

        if joined {
            ctx.reply_widget_hidden(ui::ok(
                "Entry confirmed",
                format!(
                    "You are in for **{}**. Winners are drawn {}.",
                    updated.prize,
                    ui::relative(updated.ends_at)
                ),
            ))
        } else {
            ctx.reply_widget_hidden(ui::info(
                "Entry withdrawn",
                "You are no longer taking part in this giveaway.",
            ))
        }
    }
}

pub struct GiveawayEndHandler {
    pub service: GiveawayService,
}

impl ComponentHandler for GiveawayEndHandler {
    fn matches(&self, custom_id: &str) -> bool {
        custom_id == END_ID
    }

    fn execute(&self, ctx: &CommandContext) -> Result<()> {
        let Some(message_id) = ctx.message_id() else {
            return ctx
                .reply_widget_hidden(ui::fail("Missing giveaway", "No giveaway attached here."));
        };
        if !ctx.has_permission(PERM_ADMINISTRATOR) {
            return ctx.reply_widget_hidden(ui::fail(
                "Not allowed",
                "Only administrators can end a giveaway.",
            ));
        }
        let Some(record) = self.service.giveaways.get(message_id) else {
            return ctx.reply_widget_hidden(ui::warn(
                "Giveaway unavailable",
                "This giveaway is no longer tracked by the bot.",
            ));
        };
        if record.ended {
            return ctx.reply_widget_hidden(ui::warn(
                "Giveaway closed",
                "This giveaway has already ended.",
            ));
        }

        finalize(&self.service, message_id);
        ctx.reply_widget_hidden(ui::ok("Giveaway closed", "Winners have been drawn."))
    }
}

pub struct GiveawayRerollHandler {
    pub service: GiveawayService,
}

impl ComponentHandler for GiveawayRerollHandler {
    fn matches(&self, custom_id: &str) -> bool {
        custom_id == REROLL_ID
    }

    fn execute(&self, ctx: &CommandContext) -> Result<()> {
        let (Some(message_id), Some(guild_id), Some(author)) =
            (ctx.message_id(), ctx.guild_id(), ctx.author())
        else {
            return ctx
                .reply_widget_hidden(ui::fail("Missing giveaway", "No giveaway attached here."));
        };
        if !ctx.has_permission(PERM_ADMINISTRATOR) {
            return ctx.reply_widget_hidden(ui::fail(
                "Not allowed",
                "Only administrators can reroll a giveaway.",
            ));
        }
        let Some(record) = self.service.giveaways.get(message_id) else {
            return ctx.reply_widget_hidden(ui::warn(
                "Giveaway unavailable",
                "This giveaway is no longer tracked by the bot.",
            ));
        };
        if record.entrants.is_empty() {
            return ctx.reply_widget_hidden(ui::warn(
                "Nothing to reroll",
                "Nobody entered this giveaway.",
            ));
        }

        let previous: Vec<String> = record.winners.clone();
        let pool: Vec<String> = record
            .entrants
            .iter()
            .filter(|id| !previous.contains(id))
            .cloned()
            .collect();
        let winners = draw(
            if pool.is_empty() {
                &record.entrants
            } else {
                &pool
            },
            record.winner_count,
        );

        let Some(updated) = self
            .service
            .giveaways
            .update(message_id, |r| r.winners = winners.clone())
        else {
            return ctx.reply_widget_hidden(ui::warn(
                "Giveaway unavailable",
                "This giveaway is no longer tracked by the bot.",
            ));
        };

        let theme = self.service.theme(guild_id);
        refresh(ctx, &updated, &theme);
        let issued = grant_rewards(&self.service, &updated, &winners);
        announce(&self.service, &updated, &theme, true, issued);

        self.service.logger.audit(
            guild_id,
            AuditEntry::new(LOG_GIVEAWAYS, "Giveaway rerolled")
                .accent(ui::WARNING)
                .actor(&author.id)
                .field("Prize", updated.prize.clone())
                .field(
                    "Winners",
                    winners
                        .iter()
                        .map(|id| ui::user(id))
                        .collect::<Vec<_>>()
                        .join(", "),
                ),
        );

        ctx.reply_widget_hidden(ui::ok("Reroll done", "New winners have been announced."))
    }
}

pub fn schedule_end(service: GiveawayService, message_id: String, ends_at: u64) {
    let scheduler = service.scheduler.clone();
    scheduler.schedule_at(ends_at, move || {
        finalize(&service, &message_id);
    });
}

fn finalize(service: &GiveawayService, message_id: &str) {
    let Some(record) = service.giveaways.get(message_id) else {
        return;
    };
    if record.ended {
        return;
    }

    let winners = draw(&record.entrants, record.winner_count);
    let Some(updated) = service.giveaways.update(message_id, |r| {
        r.ended = true;
        r.winners = winners.clone();
        if r.ends_at > now_secs() {
            r.ends_at = now_secs();
        }
    }) else {
        return;
    };

    let theme = service.theme(&updated.guild_id);
    let payload = MessagePayload::widget(widget(&updated, &theme)).no_mentions();
    if let Err(e) = service
        .rest
        .edit_message(&updated.channel_id, message_id, &payload)
    {
        logs::error("giveaway", format!("failed to close giveaway: {e}"));
    }

    let issued = grant_rewards(service, &updated, &winners);
    announce(service, &updated, &theme, false, issued);

    service.logger.audit(
        &updated.guild_id,
        AuditEntry::new(LOG_GIVEAWAYS, "Giveaway ended")
            .accent(theme.accent)
            .actor(&updated.host_id)
            .target(ui::channel(&updated.channel_id))
            .field("Prize", updated.prize.clone())
            .field("Entries", updated.entrants.len().to_string())
            .field("Reward", reward_label(&updated))
            .field(
                "Winners",
                if updated.winners.is_empty() {
                    "none".to_string()
                } else {
                    updated
                        .winners
                        .iter()
                        .map(|id| ui::user(id))
                        .collect::<Vec<_>>()
                        .join(", ")
                },
            ),
    );
    logs::info("giveaway", format!("closed giveaway {message_id}"));
}

fn grant_rewards(
    service: &GiveawayService,
    record: &GiveawayRecord,
    winners: &[String],
) -> Vec<(String, bool)> {
    if record.reward_kind != REWARD_LICENSE || winners.is_empty() {
        return Vec::new();
    }

    let plan = record
        .reward_plan
        .clone()
        .unwrap_or_else(|| "monthly".to_string());
    let theme = service.theme(&record.guild_id);
    let mut outcome = Vec::new();

    for winner in winners {
        let issued = service.licenses.issue(
            IssueRequest::new(service.product.clone(), plan.clone())
                .duration(record.reward_days.map(|days| days * 86_400))
                .machines(1)
                .owner(Some(winner.clone()))
                .note(Some(format!("Giveaway {}", record.message_id)))
                .issued_by(Some(record.host_id.clone())),
        );

        let license = issued.license;
        let body = vec![
            ui::subtitle("You won a licence"),
            format!("**{}**", issued.key),
            ui::kv("Product", license.product.clone()),
            ui::kv("Plan", license.plan.clone()),
            ui::kv(
                "Duration",
                match license.duration_secs {
                    Some(duration) => format!("{} once activated", format_duration(duration)),
                    None => "never expires".to_string(),
                },
            ),
            ui::note(format!(
                "Won in the giveaway for {}. Paste the key in the launcher to bind it to your machine.",
                record.prize
            )),
        ];

        let delivered = service
            .rest
            .send_direct_message(
                winner,
                &MessagePayload::widget(ui::panel(theme.accent, vec![ui::lines(body)]))
                    .no_mentions(),
            )
            .is_ok();

        if !delivered {
            logs::warn(
                "giveaway",
                format!(
                    "licence {} could not be delivered to {winner}",
                    license.key_prefix
                ),
            );
        }

        service.giveaways.update(&record.message_id, |r| {
            r.issued.push((winner.clone(), license.key_prefix.clone()));
        });
        outcome.push((winner.clone(), delivered));
    }

    outcome
}

fn announce(
    service: &GiveawayService,
    record: &GiveawayRecord,
    theme: &Theme,
    reroll: bool,
    issued: Vec<(String, bool)>,
) {
    let heading = if reroll {
        "Giveaway rerolled"
    } else {
        "Giveaway results"
    };

    let mut lines = vec![ui::subtitle(heading)];

    if record.winners.is_empty() {
        lines.push("Nobody entered, no winner could be drawn.".to_string());
    } else {
        lines.push(format!(
            "{} won **{}**.",
            record
                .winners
                .iter()
                .map(|id| ui::user(id))
                .collect::<Vec<_>>()
                .join(", "),
            record.prize
        ));
    }

    if !issued.is_empty() {
        let undelivered: Vec<String> = issued
            .iter()
            .filter(|(_, delivered)| !delivered)
            .map(|(id, _)| ui::user(id))
            .collect();

        lines.push(if undelivered.is_empty() {
            ui::note("Licence keys were sent by direct message.")
        } else {
            ui::note(format!(
                "Licence keys were sent by direct message. {} must open their DMs, the staff will hand the key over.",
                undelivered.join(", ")
            ))
        });
    }

    lines.push(ui::note(format!("{} entries", record.entrants.len())));

    let components = ui::panel(
        theme.accent,
        vec![ui::section_button(
            lines,
            Button::link("Open giveaway", record.link()),
        )],
    );

    let payload =
        MessagePayload::widget(components).mentions(AllowedMentions::users(record.winners.clone()));

    if let Err(e) = service.rest.create_message(&record.channel_id, &payload) {
        logs::error("giveaway", format!("announcement failed: {e}"));
    }
}

fn draw(entrants: &[String], count: u32) -> Vec<String> {
    if entrants.is_empty() {
        return Vec::new();
    }

    let mut pool: Vec<String> = entrants.to_vec();
    let mut winners = Vec::new();
    let take = (count as usize).min(pool.len());

    for _ in 0..take {
        let index = random_index(pool.len());
        winners.push(pool.remove(index));
    }

    winners
}

fn random_index(len: usize) -> usize {
    let mut buf = [0u8; 8];
    getrandom::fill(&mut buf).expect("getrandom failed");
    (u64::from_le_bytes(buf) as usize) % len
}
