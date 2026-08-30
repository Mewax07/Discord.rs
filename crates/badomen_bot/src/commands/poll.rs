use std::sync::Arc;

use discord::commands::{CommandContext, SlashCommand};
use discord::error::Result;
use discord::models::{
    Button, CommandDefinition, CommandOption, MessagePayload, PollRequest, PERM_ADMINISTRATOR,
    POLL_ANSWER_LEN, POLL_MAX_ANSWERS, POLL_MAX_HOURS, POLL_MIN_HOURS, POLL_QUESTION_LEN,
};
use discord::rest::RestClient;

use crate::logs::{self, AuditEntry, Logger};
use crate::scheduler::Scheduler;
use crate::storage::{ConfigStore, PollRecord, PollStore, LOG_POLLS};
use crate::ui::{self, Theme};
use crate::util::{format_duration, now_secs, parse_duration, truncate};

const MIN_OPTIONS: usize = 2;
const RESULT_DELAY: u64 = 45;

pub struct PollCommand {
    pub polls: Arc<PollStore>,
    pub config: Arc<ConfigStore>,
    pub scheduler: Arc<Scheduler>,
    pub rest: Arc<RestClient>,
    pub logger: Arc<Logger>,
}

impl SlashCommand for PollCommand {
    fn definition(&self) -> CommandDefinition {
        CommandDefinition::new("poll", "Native Discord polls")
            .guild_only()
            .required_permissions(PERM_ADMINISTRATOR)
            .option(
                CommandOption::subcommand("create", "Publish a poll")
                    .option(
                        CommandOption::string("question", "What are you asking?").required(true),
                    )
                    .option(
                        CommandOption::string("choices", "Choices separated by | or , (2 to 10)")
                            .required(true),
                    )
                    .option(
                        CommandOption::string("duration", "How long it runs, such as 6h, 2d or 1w")
                            .required(true),
                    )
                    .option(CommandOption::boolean(
                        "multiple",
                        "Allow everyone to pick several choices",
                    )),
            )
            .option(
                CommandOption::subcommand("end", "Close a running poll immediately").option(
                    CommandOption::string("message", "Poll message link or identifier")
                        .required(true),
                ),
            )
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
                "Only administrators can manage polls.",
            ));
        }

        match ctx.subcommand() {
            Some("create") => self.create(ctx, guild_id),
            Some("end") => self.end(ctx, guild_id),
            _ => ctx.reply_widget_hidden(ui::fail(
                "Unknown action",
                "This subcommand does not exist.",
            )),
        }
    }
}

impl PollCommand {
    fn create(&self, ctx: &CommandContext, guild_id: &str) -> Result<()> {
        let Some(channel_id) = ctx.channel_id() else {
            return ctx.reply_widget_hidden(ui::fail("Missing channel", "No channel context."));
        };
        let Some(author) = ctx.author() else {
            return ctx.reply_widget_hidden(ui::fail("Unknown user", "Unable to identify you."));
        };
        let (Some(question), Some(raw_choices), Some(raw_duration)) = (
            ctx.option_string("question"),
            ctx.option_string("choices"),
            ctx.option_string("duration"),
        ) else {
            return ctx.reply_widget_hidden(ui::fail(
                "Missing input",
                "A question, the choices and a duration are all required.",
            ));
        };

        let separator = if raw_choices.contains('|') { '|' } else { ',' };
        let options: Vec<String> = raw_choices
            .split(separator)
            .map(|choice| truncate(choice, POLL_ANSWER_LEN))
            .filter(|choice| !choice.is_empty())
            .collect();

        if options.len() < MIN_OPTIONS || options.len() > POLL_MAX_ANSWERS {
            return ctx.reply_widget_hidden(ui::fail(
                "Invalid choices",
                format!(
                    "Provide between **{MIN_OPTIONS}** and **{POLL_MAX_ANSWERS}** choices, separated by {} or {}. Each choice is limited to **{POLL_ANSWER_LEN}** characters.",
                    ui::code("|"),
                    ui::code(",")
                ),
            ));
        }

        let Some(duration) = parse_duration(raw_duration) else {
            return ctx.reply_widget_hidden(ui::fail(
                "Invalid duration",
                format!(
                    "Use a value such as {}, {} or {}.",
                    ui::code("6h"),
                    ui::code("2d"),
                    ui::code("1w")
                ),
            ));
        };

        let hours = (duration.div_ceil(3_600) as u32).clamp(POLL_MIN_HOURS, POLL_MAX_HOURS);
        let question = truncate(question, POLL_QUESTION_LEN);
        let multi = ctx.option_boolean("multiple").unwrap_or(false);

        let request = PollRequest::new(question.clone(), options.clone(), hours).multiselect(multi);
        let message = ctx.send(channel_id, MessagePayload::poll(request))?;

        let now = now_secs();
        let ends_at = now + (hours as u64) * 3_600;
        let record = PollRecord {
            guild_id: guild_id.to_string(),
            channel_id: channel_id.to_string(),
            message_id: message.id.clone(),
            question: question.clone(),
            options: options.clone(),
            author_id: author.id.clone(),
            multi,
            created_at: now,
            ends_at,
            ended: false,
        };
        self.polls.insert(&message.id, record.clone());

        schedule_end(
            self.polls.clone(),
            self.rest.clone(),
            self.config.clone(),
            self.logger.clone(),
            message.id.clone(),
            ends_at,
            &self.scheduler,
        );

        let theme = Theme::from_brand(&self.config.brand(guild_id));
        self.logger.audit(
            guild_id,
            AuditEntry::new(LOG_POLLS, "Poll published")
                .accent(theme.accent)
                .actor(&author.id)
                .target(ui::channel(channel_id))
                .field("Question", question)
                .field("Choices", options.len().to_string())
                .field(
                    "Mode",
                    if multi {
                        "Multiple choice"
                    } else {
                        "Single choice"
                    },
                )
                .field("Closes", ui::full_date(ends_at)),
        );
        logs::info("poll", format!("published in {channel_id} for {hours}h"));

        ctx.reply(
            MessagePayload::widget(vec![ui::section_button(
                vec![
                    ui::subtitle("Poll published"),
                    ui::note(format!(
                        "Native Discord poll · closes in {} · {}",
                        format_duration((hours as u64) * 3_600),
                        if multi {
                            "multiple choice"
                        } else {
                            "single choice"
                        }
                    )),
                ],
                Button::link("Open poll", record.link()),
            )])
            .ephemeral(),
        )
    }

    fn end(&self, ctx: &CommandContext, guild_id: &str) -> Result<()> {
        let Some(raw) = ctx.option_string("message") else {
            return ctx.reply_widget_hidden(ui::fail(
                "Missing input",
                "Paste the poll message link or its identifier.",
            ));
        };
        let Some(message_id) = message_id_of(raw) else {
            return ctx.reply_widget_hidden(ui::fail(
                "Invalid reference",
                "Use the message link (right click, Copy Message Link) or the raw identifier.",
            ));
        };

        let channel_id = self
            .polls
            .get(&message_id)
            .map(|record| record.channel_id)
            .or_else(|| ctx.channel_id().map(String::from));
        let Some(channel_id) = channel_id else {
            return ctx.reply_widget_hidden(ui::fail("Missing channel", "No channel context."));
        };

        match ctx.expire_poll(&channel_id, &message_id) {
            Ok(_) => {
                self.polls.update(&message_id, |r| {
                    r.ended = true;
                    r.ends_at = now_secs();
                });

                if let Some(author) = ctx.author() {
                    self.logger.audit(
                        guild_id,
                        AuditEntry::new(LOG_POLLS, "Poll closed early")
                            .accent(ui::WARNING)
                            .actor(&author.id)
                            .target(ui::channel(&channel_id))
                            .field("Message", ui::code(&message_id)),
                    );
                }

                ctx.reply_widget_hidden(ui::ok(
                    "Poll closed",
                    "Discord published the final results on the poll itself.",
                ))
            }
            Err(e) => {
                logs::error("poll", format!("expire failed: {e}"));
                ctx.reply_widget_hidden(ui::fail(
                    "Cannot close this poll",
                    "The message was not found, is not a poll, or has already ended.",
                ))
            }
        }
    }
}

fn message_id_of(raw: &str) -> Option<String> {
    let candidate = raw
        .trim()
        .rsplit('/')
        .next()
        .unwrap_or_default()
        .split('?')
        .next()
        .unwrap_or_default();

    (candidate.len() >= 15 && candidate.chars().all(|c| c.is_ascii_digit()))
        .then(|| candidate.to_string())
}

pub fn schedule_end(
    polls: Arc<PollStore>,
    rest: Arc<RestClient>,
    config: Arc<ConfigStore>,
    logger: Arc<Logger>,
    message_id: String,
    ends_at: u64,
    scheduler: &Scheduler,
) {
    scheduler.schedule_at(ends_at + RESULT_DELAY, move || {
        report(&polls, &rest, &config, &logger, &message_id);
    });
}

fn report(
    polls: &PollStore,
    rest: &RestClient,
    config: &ConfigStore,
    logger: &Logger,
    message_id: &str,
) {
    let Some(record) = polls.get(message_id) else {
        return;
    };
    if record.ended {
        return;
    }

    let message = match rest.get_message(&record.channel_id, message_id) {
        Ok(message) => message,
        Err(e) => {
            logs::error("poll", format!("cannot read poll {message_id}: {e}"));
            polls.update(message_id, |r| r.ended = true);
            return;
        }
    };

    polls.update(message_id, |r| r.ended = true);

    let Some(poll) = message.poll else {
        return;
    };

    let tally = poll.tally();
    let total = poll.total_votes().max(1) as usize;
    let best = tally.iter().map(|(_, count)| *count).max().unwrap_or(0);

    let breakdown = tally
        .iter()
        .map(|(label, count)| {
            let count = *count as usize;
            format!(
                "{} {} · {}% · {count}",
                ui::progress(count, total, 12),
                label,
                ui::percent(count, total)
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    let outcome = if best == 0 {
        "No vote was cast.".to_string()
    } else {
        let winners: Vec<String> = tally
            .iter()
            .filter(|(_, count)| *count == best)
            .map(|(label, _)| label.clone())
            .collect();
        if winners.len() == 1 {
            format!("**{}** wins the vote.", winners[0])
        } else {
            format!("Tie between **{}**.", winners.join("**, **"))
        }
    };

    let theme = Theme::from_brand(&config.brand(&record.guild_id));
    logger.audit(
        &record.guild_id,
        AuditEntry::new(LOG_POLLS, "Poll ended")
            .accent(theme.accent)
            .actor(&record.author_id)
            .target(ui::channel(&record.channel_id))
            .field("Question", record.question.clone())
            .field("Votes", poll.total_votes().to_string())
            .field("Outcome", outcome)
            .detail(breakdown),
    );
    logs::info("poll", format!("results logged for {message_id}"));
}
