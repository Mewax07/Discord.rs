use std::sync::Arc;

use discord::commands::{CommandContext, ComponentHandler, SlashCommand};
use discord::error::Result;
use discord::models::{
    ActionRow, CommandDefinition, CommandOption, Embed, InteractionResponse, PermissionOverwrite,
    SelectMenu, SelectOption, PERM_SEND_MESSAGES, PERM_VIEW_CHANNEL,
};

use crate::storage::ConfigStore;

const PURPLE: u32 = 0x8B5CF6;
const CATEGORY_SELECT_ID: &str = "ticket_category_select";

struct TicketCategory {
    key: &'static str,
    label: &'static str,
    description: &'static str,
    emoji: &'static str,
}

const CATEGORIES: &[TicketCategory] = &[
    TicketCategory {
        key: "bug",
        label: "Report a bug",
        description: "A technical issue with BadOmen Visual or Nouga Launcher",
        emoji: "🐛",
    },
    TicketCategory {
        key: "license",
        label: "License key",
        description: "Question or issue with your license",
        emoji: "🔑",
    },
    TicketCategory {
        key: "feature",
        label: "Suggestion",
        description: "Propose an idea for BadOmen Visual or Nouga Launcher",
        emoji: "💡",
    },
    TicketCategory {
        key: "faq",
        label: "General question",
        description: "Any other questions",
        emoji: "❓",
    },
];

pub struct TicketCommand;

impl SlashCommand for TicketCommand {
    fn definition(&self) -> CommandDefinition {
        CommandDefinition::new("ticket", "Ticket system management")
            .option(CommandOption::subcommand(
                "setup",
                "Post the ticket opening panel.",
            ))
            .option(CommandOption::subcommand(
                "close",
                "Close the current ticket",
            ))
    }

    fn execute(&self, ctx: &CommandContext) -> Result<()> {
        match ctx.subcommand() {
            Some("setup") => setup(ctx),
            Some("close") => close(ctx),
            _ => ctx.reply("Unknown subcommand."),
        }
    }
}

fn setup(ctx: &CommandContext) -> Result<()> {
    let Some(channel_id) = ctx.channel_id() else {
        return ctx.reply("This command must be used in a channel.");
    };

    let embed = Embed::new()
        .title("🎫 Support BadOmen Visual & Nouga Launcher")
        .description("Select a category below to open a ticket with the team.")
        .color(PURPLE)
        .image("https://raw.githubusercontent.com/Mewax07/Discord.rs/refs/heads/main/assets/main/ticket_banner.png")
        .thumbnail("https://raw.githubusercontent.com/Mewax07/Discord.rs/refs/heads/main/assets/main/bad_omen_logo.png")
        .footer("BadOmen Visual & Nouga Launcher Support");

    let options = CATEGORIES
        .iter()
        .map(|c| {
            SelectOption::new(c.label, c.key)
                .description(c.description)
                .emoji(c.emoji)
        })
        .collect();

    let menu = SelectMenu::new(CATEGORY_SELECT_ID, options).placeholder("Choose a category...");

    ctx.send_channel_message(channel_id, None, vec![embed], vec![ActionRow::select(menu)])?;

    ctx.reply_response(InteractionResponse::message("✅ Panel published.").ephemeral())
}

fn close(ctx: &CommandContext) -> Result<()> {
    ctx.reply("🔒 Closing the ticket...")?;
    if let Some(channel_id) = ctx.channel_id() {
        ctx.delete_channel(channel_id)?;
    }
    Ok(())
}

pub struct TicketCategoryHandler {
    pub config: Arc<ConfigStore>,
}

impl ComponentHandler for TicketCategoryHandler {
    fn matches(&self, custom_id: &str) -> bool {
        custom_id == CATEGORY_SELECT_ID
    }

    fn execute(&self, ctx: &CommandContext) -> Result<()> {
        let Some(guild_id) = ctx.guild_id() else {
            return ctx.reply("This action must be performed on a server.");
        };
        let Some(author) = ctx.author() else {
            return ctx.reply("Unable to identify the user.");
        };
        let Some(category_key) = ctx.selected_value() else {
            return ctx.reply("Invalid category.");
        };
        let Some(category) = CATEGORIES.iter().find(|c| c.key == category_key) else {
	            return ctx.reply("Unknown category.");
        };

        let cfg = self.config.get(guild_id);
        let number = self.config.next_ticket_number(guild_id);
        let name = format!("{}-{number:04}", category.key);

        let mut overwrites = vec![
            PermissionOverwrite::deny_everyone(guild_id, PERM_VIEW_CHANNEL),
            PermissionOverwrite::allow_member(&author.id, PERM_VIEW_CHANNEL | PERM_SEND_MESSAGES),
        ];
        if let Some(staff_role) = &cfg.staff_role_id {
            overwrites.push(PermissionOverwrite::allow_role(
                staff_role,
                PERM_VIEW_CHANNEL | PERM_SEND_MESSAGES,
            ));
        }

        let channel = ctx.create_channel(
            guild_id,
            &name,
            cfg.ticket_category_id.as_deref(),
            overwrites,
        )?;

        let intro = Embed::new()
            .title(format!("{} {}", category.emoji, category.label))
            .description(format!(
                "Ticket opened by {}.\nA staff member will reply to you shortly.",
                author.mention()
            ))
            .color(PURPLE)
            .footer("BadOmen Visual & Nouga Launcher Support");

        ctx.send_channel_message(&channel.id, None, vec![intro], vec![])?;

        ctx.reply_response(
            InteractionResponse::message(format!("🎫 Ticket created: {}", channel.mention()))
                .ephemeral(),
        )
    }
}
