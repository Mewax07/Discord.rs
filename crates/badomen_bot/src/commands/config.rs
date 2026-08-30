use std::sync::Arc;

use discord::commands::{CommandContext, SlashCommand};
use discord::error::Result;
use discord::models::{CommandDefinition, CommandOption};

use crate::storage::ConfigStore;

pub struct ConfigCommand {
    pub config: Arc<ConfigStore>,
}

impl SlashCommand for ConfigCommand {
    fn definition(&self) -> CommandDefinition {
        CommandDefinition::new("config", "Configure the bot for this server")
            .option(
                CommandOption::subcommand("ticket-category", "Category in which to create tickets")
                    .option(CommandOption::channel("category", "Lounge category").required(true)),
            )
            .option(
                CommandOption::subcommand("staff-role", "Role with access to tickets")
                    .option(CommandOption::role("role", "Staff role").required(true)),
            )
    }

    fn execute(&self, ctx: &CommandContext) -> Result<()> {
        let Some(guild_id) = ctx.guild_id() else {
            return ctx.reply("This command must be used on a server.");
        };

        match ctx.subcommand() {
            Some("ticket-category") => {
                let Some(channel) = ctx.option_channel("category") else {
                    return ctx.reply("Lounge not found.");
                };
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
                ctx.reply(format!("Staff role defined for {mention}."))
            }
            _ => ctx.reply("Unknown subcommand."),
        }
    }
}
