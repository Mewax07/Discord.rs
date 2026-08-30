use std::sync::Arc;

use discord::commands::{CommandContext, ComponentHandler, SlashCommand};
use discord::error::Result;
use discord::models::{
    ActionRow, CommandDefinition, CommandOption, Embed, InteractionResponse, PermissionOverwrite,
    SelectMenu, SelectOption, TextInput, TextInputStyle, PERM_SEND_MESSAGES, PERM_VIEW_CHANNEL,
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
        .description("Need a hand? Pick what best matches your situation below and we'll take it from there.")
        .color(PURPLE)
        .image("https://raw.githubusercontent.com/Mewax07/Discord.rs/main/assets/main/ticket_banner.gif")
        .thumbnail("https://raw.githubusercontent.com/Mewax07/Discord.rs/main/assets/main/bad_omen_logo.png")
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
    ctx.reply("Alright, closing this ticket. Thanks for reaching out!")?;
    if let Some(channel_id) = ctx.channel_id() {
        ctx.delete_channel(channel_id)?;
    }
    Ok(())
}

fn build_modal_fields(category_key: &str) -> Vec<ActionRow> {
    match category_key {
        "bug" => vec![
            ActionRow::input(
                TextInput::new(
                    "bug_product",
                    "Product (BadOmen or Nouga)",
                    TextInputStyle::Short,
                )
                .placeholder("BadOmen Visual / Nouga Launcher"),
            ),
            ActionRow::input(
                TextInput::new("bug_version", "Version", TextInputStyle::Short)
                    .placeholder("e.g. 1.4.2"),
            ),
            ActionRow::input(
                TextInput::new("bug_specs", "OS / PC specs", TextInputStyle::Short)
                    .placeholder("Windows 11, RTX 3060..."),
            ),
            ActionRow::input(TextInput::new(
                "bug_steps",
                "Steps to reproduce",
                TextInputStyle::Paragraph,
            )),
            ActionRow::input(
                TextInput::new(
                    "bug_logs",
                    "Logs or error message",
                    TextInputStyle::Paragraph,
                )
                .required(false)
                .placeholder("Optional - paste a link or the error text"),
            ),
        ],
        "license" => vec![
            ActionRow::input(TextInput::new(
                "license_username",
                "Your username",
                TextInputStyle::Short,
            )),
            ActionRow::input(
                TextInput::new("license_hwid", "HWID", TextInputStyle::Short)
                    .required(false)
                    .placeholder("Optional"),
            ),
        ],
        "feature" => vec![
            ActionRow::input(TextInput::new(
                "feature_title",
                "Short title",
                TextInputStyle::Short,
            )),
            ActionRow::input(TextInput::new(
                "feature_description",
                "Describe your idea",
                TextInputStyle::Paragraph,
            )),
        ],
        _ => vec![ActionRow::input(TextInput::new(
            "faq_question",
            "Your question",
            TextInputStyle::Paragraph,
        ))],
    }
}

fn build_intro_embed(ctx: &CommandContext, category: &TicketCategory, author_name: &str) -> Embed {
    let mut embed = Embed::new()
        .title(format!("{} {}", category.emoji, category.label))
        .color(PURPLE)
        .footer("BadOmen Visual & Nouga Launcher Support");

    match category.key {
        "bug" => {
            embed = embed.description(format!(
                "Hey {author_name}, thanks for reporting this! The team will take a look and get back to you soon."
            ));
            if let Some(v) = ctx.modal_value("bug_product") {
                embed = embed.field("Product", v, true);
            }
            if let Some(v) = ctx.modal_value("bug_version") {
                embed = embed.field("Version", v, true);
            }
            if let Some(v) = ctx.modal_value("bug_specs") {
                embed = embed.field("System", v, true);
            }
            if let Some(v) = ctx.modal_value("bug_steps") {
                embed = embed.field("Steps to reproduce", v, false);
            }
            if let Some(v) = ctx.modal_value("bug_logs") {
                if !v.is_empty() {
                    embed = embed.field("Logs", v, false);
                }
            }
        }
        "license" => {
            embed = embed.description(format!(
                "Hi {author_name}, we'll help you sort this out with your license shortly."
            ));
            if let Some(v) = ctx.modal_value("license_username") {
                embed = embed.field("Username", v, true);
            }
            if let Some(v) = ctx.modal_value("license_hwid") {
                if !v.is_empty() {
                    embed = embed.field("HWID", v, true);
                }
            }
        }
        "feature" => {
            embed = embed.description(format!("Thanks for the suggestion, {author_name}! We love hearing ideas from the community."));
            if let Some(v) = ctx.modal_value("feature_title") {
                embed = embed.field("Idea", v, false);
            }
            if let Some(v) = ctx.modal_value("feature_description") {
                embed = embed.field("Details", v, false);
            }
        }
        "faq" => {
            embed = embed.description(format!(
                "Hi {author_name}, thanks for reaching out — someone will answer shortly."
            ));
            if let Some(v) = ctx.modal_value("faq_question") {
                embed = embed.field("Question", v, false);
            }
        }
        _ => {}
    }

    embed
}

pub struct TicketCategoryHandler;

impl ComponentHandler for TicketCategoryHandler {
    fn matches(&self, custom_id: &str) -> bool {
        custom_id == CATEGORY_SELECT_ID
    }

    fn execute(&self, ctx: &CommandContext) -> Result<()> {
        let Some(category_key) = ctx.selected_value() else {
            return ctx.reply("Invalid category.");
        };
        let Some(category) = CATEGORIES.iter().find(|c| c.key == category_key) else {
            return ctx.reply("Unknown category.");
        };

        let modal_id = format!("ticket_modal_{}", category.key);
        let rows = build_modal_fields(category.key);
        ctx.show_modal(modal_id, category.label, rows)
    }
}

pub struct TicketModalHandler {
    pub config: Arc<ConfigStore>,
}

impl ComponentHandler for TicketModalHandler {
    fn matches(&self, custom_id: &str) -> bool {
        custom_id.starts_with("ticket_modal_")
    }

    fn execute(&self, ctx: &CommandContext) -> Result<()> {
        let Some(guild_id) = ctx.guild_id() else {
            return ctx.reply("This action must be performed on a server.");
        };
        let Some(author) = ctx.author() else {
            return ctx.reply("Unable to identify the user.");
        };
        let Some(custom_id) = ctx.custom_id() else {
            return ctx.reply("Missing modal identifier.");
        };
        let category_key = custom_id.trim_start_matches("ticket_modal_");
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

        let category_role = cfg.category_roles.get(category.key);
        if let Some(role_id) = category_role {
            if cfg.staff_role_id.as_deref() != Some(role_id.as_str()) {
                overwrites.push(PermissionOverwrite::allow_role(
                    role_id,
                    PERM_VIEW_CHANNEL | PERM_SEND_MESSAGES,
                ));
            }
        }

        let channel = match ctx.create_channel(
            guild_id,
            &name,
            cfg.ticket_category_id.as_deref(),
            overwrites,
        ) {
            Ok(channel) => channel,
            Err(_) => {
                return ctx.reply_response(
                    InteractionResponse::message(
                        "Ticket creation failed. The configured ticket category may be invalid — ask an admin to run /config ticket-category again.",
                    )
                    .ephemeral(),
                );
            }
        };

        let embed = build_intro_embed(ctx, category, author.display_name());
        let ping = category_role
            .map(|id| format!(" <@&{id}>"))
            .unwrap_or_default();
        let content = format!("{}{}", author.mention(), ping);

        ctx.send_channel_message(&channel.id, Some(&content), vec![embed], vec![])?;

        ctx.reply_response(
            InteractionResponse::message(format!(
                "Your ticket is ready: {}. Someone from the team will be with you shortly!",
                channel.mention()
            ))
            .ephemeral(),
        )
    }
}
