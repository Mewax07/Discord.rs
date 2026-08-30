use std::sync::Arc;

use discord::commands::{CommandContext, SlashCommand};
use discord::error::Result;
use discord::models::{CommandDefinition, CommandOption, CHANNEL_TYPE_GUILD_CATEGORY};

use crate::storage::ConfigStore;

pub struct ConfigCommand {
    pub config: Arc<ConfigStore>,
}

impl SlashCommand for ConfigCommand {
    fn definition(&self) -> CommandDefinition {
        CommandDefinition::new("config", "Configure the bot for this server")
            .option(
                CommandOption::subcommand(
                    "ticket-category",
                    "Category where tickets will be created",
                )
                .option(
                    CommandOption::channel("category", "Category channel")
                        .required(true)
                        .channel_types(vec![CHANNEL_TYPE_GUILD_CATEGORY]),
                ),
            )
            .option(
                CommandOption::subcommand("staff-role", "Role with access to tickets")
                    .option(CommandOption::role("role", "Staff role").required(true)),
            )
            .option(
                CommandOption::subcommand("category-role", "Role pinged for a ticket category")
                    .option(
                        CommandOption::string("category", "Ticket category")
                            .required(true)
                            .choice("Report a bug", "bug")
                            .choice("License key", "license")
                            .choice("Suggestion", "feature")
                            .choice("General question", "faq"),
                    )
                    .option(CommandOption::role("role", "Role to ping").required(true)),
            )
    }

    fn execute(&self, ctx: &CommandContext) -> Result<()> {
        let Some(guild_id) = ctx.guild_id() else {
            return ctx.reply("This command must be used in a server.");
        };

        match ctx.subcommand() {
            Some("ticket-category") => {
                let Some(channel) = ctx.option_channel("category") else {
                    return ctx.reply("Channel not found.");
                };
                if !channel.is_category() {
                    return ctx.reply("The selected channel must be a category.");
                }
                let (id, mention) = (channel.id.clone(), channel.mention());
                self.config
                    .update(guild_id, |c| c.ticket_category_id = Some(id));
                ctx.reply(format!("Ticket category set to {mention}."))
            }
            Some("staff-role") => {
                let Some(role) = ctx.option_role("role") else {
                    return ctx.reply("Role not found.");
                };
                let (id, mention) = (role.id.clone(), role.mention());
                self.config.update(guild_id, |c| c.staff_role_id = Some(id));
                ctx.reply(format!("Staff role set to {mention}."))
            }
            Some("category-role") => {
                let Some(category) = ctx.option_string("category") else {
                    return ctx.reply("Missing category.");
                };
                let Some(role) = ctx.option_role("role") else {
                    return ctx.reply("Role not found.");
                };
                let (category, id, mention) =
                    (category.to_string(), role.id.clone(), role.mention());
                self.config.update(guild_id, |c| {
                    c.category_roles.insert(category, id);
                });
                ctx.reply(format!("Tickets in this category will now ping {mention}."))
            }
            _ => ctx.reply("Unknown subcommand."),
        }
    }
}
