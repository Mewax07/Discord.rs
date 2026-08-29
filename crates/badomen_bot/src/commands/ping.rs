use discord::{
    commands::{CommandContext, SlashCommand},
    models::CommandDefinition,
    Result,
};

pub struct PingCommand;

impl SlashCommand for PingCommand {
    fn definition(&self) -> CommandDefinition {
        CommandDefinition::new("ping", "Ping for check bot connection")
    }

    fn execute(&self, ctx: &CommandContext) -> Result<()> {
        ctx.reply("pong")
    }
}
