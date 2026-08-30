use std::sync::Arc;

use discord::commands::{CommandContext, SlashCommand};
use discord::error::Result;
use discord::models::{CommandDefinition, CommandOption, Message, PERM_ADMINISTRATOR};

use crate::logs::{self, AuditEntry, Logger};
use crate::storage::{ConfigStore, LOG_CONFIG};
use crate::ui::{self, Theme};
use crate::util::{now_secs, plural};

const DISCORD_EPOCH_MS: u64 = 1_420_070_400_000;
const BULK_MAX_AGE: u64 = 14 * 86_400;
const SLOW_DELETE_CAP: usize = 15;
const SCAN_LIMIT: u16 = 100;

pub struct ClearCommand {
    pub config: Arc<ConfigStore>,
    pub logger: Arc<Logger>,
}

impl SlashCommand for ClearCommand {
    fn definition(&self) -> CommandDefinition {
        CommandDefinition::new("clear", "Delete recent messages from this channel")
            .guild_only()
            .required_permissions(PERM_ADMINISTRATOR)
            .option(
                CommandOption::integer("amount", "How many messages to remove, 1 to 100")
                    .required(true)
                    .min_value(1)
                    .max_value(100),
            )
            .option(CommandOption::user(
                "member",
                "Only delete the messages of this member",
            ))
            .option(CommandOption::boolean(
                "bots",
                "Only delete messages posted by bots",
            ))
            .option(CommandOption::string("reason", "Why are they removed"))
    }

    fn execute(&self, ctx: &CommandContext) -> Result<()> {
        let Some(guild_id) = ctx.guild_id() else {
            return ctx.reply_widget_hidden(ui::fail(
                "Server only",
                "This command can only be used inside a server.",
            ));
        };
        let Some(channel_id) = ctx.channel_id() else {
            return ctx.reply_widget_hidden(ui::fail("Missing channel", "No channel context."));
        };
        if !ctx.has_permission(PERM_ADMINISTRATOR) {
            return ctx.reply_widget_hidden(ui::fail(
                "Not allowed",
                "Only administrators can purge a channel.",
            ));
        }

        let amount = ctx.option_integer("amount").unwrap_or(0).clamp(1, 100) as usize;
        let member = ctx.option_user("member");
        let bots_only = ctx.option_boolean("bots").unwrap_or(false);
        let reason = ctx.option_string("reason").map(String::from);

        let scanned = ctx
            .recent_messages(channel_id, SCAN_LIMIT)
            .unwrap_or_default();
        let targets: Vec<&Message> = scanned
            .iter()
            .filter(|message| !message.pinned)
            .filter(|message| match member {
                Some(user) => message.author.id == user.id,
                None => true,
            })
            .filter(|message| !bots_only || message.author.bot)
            .take(amount)
            .collect();

        if targets.is_empty() {
            return ctx.reply_widget_hidden(ui::info(
                "Nothing to delete",
                "No matching message was found in the last 100 of this channel.",
            ));
        }

        let now = now_secs();
        let (fresh, old): (Vec<&Message>, Vec<&Message>) = targets
            .iter()
            .partition(|message| now.saturating_sub(created_at(&message.id)) < BULK_MAX_AGE);

        let mut deleted = 0usize;
        let mut failed = 0usize;

        let fresh_ids: Vec<String> = fresh.iter().map(|message| message.id.clone()).collect();
        match fresh_ids.len() {
            0 => {}
            1 => match ctx.delete_message(channel_id, &fresh_ids[0]) {
                Ok(()) => deleted += 1,
                Err(_) => failed += 1,
            },
            _ => match ctx.bulk_delete(channel_id, &fresh_ids) {
                Ok(()) => deleted += fresh_ids.len(),
                Err(e) => {
                    logs::error("clear", format!("bulk delete failed: {e}"));
                    failed += fresh_ids.len();
                }
            },
        }

        let mut skipped = 0usize;
        for (index, message) in old.iter().enumerate() {
            if index >= SLOW_DELETE_CAP {
                skipped = old.len() - SLOW_DELETE_CAP;
                break;
            }
            match ctx.delete_message(channel_id, &message.id) {
                Ok(()) => deleted += 1,
                Err(_) => failed += 1,
            }
        }

        let theme = Theme::from_brand(&self.config.brand(guild_id));
        if let Some(author) = ctx.author() {
            self.logger.audit(
                guild_id,
                AuditEntry::new(LOG_CONFIG, "Channel purged")
                    .accent(ui::WARNING)
                    .actor(&author.id)
                    .target(ui::channel(channel_id))
                    .field("Deleted", deleted.to_string())
                    .maybe_field("Filtered on", member.map(|user| user.mention()))
                    .maybe_field("Reason", reason.clone()),
            );
        }
        logs::info(
            "clear",
            format!("{deleted} messages removed in {channel_id}"),
        );

        let mut summary = vec![
            ui::subtitle("Channel cleaned"),
            ui::kv("Removed", plural(deleted, "message", "messages")),
        ];
        if let Some(user) = member {
            summary.push(ui::kv("Author", user.mention()));
        }
        if bots_only {
            summary.push(ui::kv("Filter", "bots only"));
        }
        if failed > 0 {
            summary.push(ui::kv("Failed", failed.to_string()));
        }
        if skipped > 0 {
            summary.push(ui::note(format!(
                "{skipped} messages older than 14 days were left untouched, Discord only removes those one by one."
            )));
        }
        if let Some(reason) = reason {
            summary.push(ui::kv("Reason", reason));
        }

        ctx.reply_widget_hidden(ui::panel(theme.accent, vec![ui::lines(summary)]))
    }
}

fn created_at(message_id: &str) -> u64 {
    message_id
        .parse::<u64>()
        .map(|id| ((id >> 22) + DISCORD_EPOCH_MS) / 1_000)
        .unwrap_or(0)
}
