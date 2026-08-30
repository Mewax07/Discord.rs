mod context;
mod registry;

pub use context::CommandContext;
pub use registry::CommandRegistry;

use crate::{
    models::{CommandChoice, CommandDefinition},
    Result,
};

pub trait SlashCommand: Send + Sync {
    fn definition(&self) -> CommandDefinition;

    fn execute(&self, ctx: &CommandContext) -> Result<()>;

    fn autocomplete(&self, _ctx: &CommandContext) -> Vec<CommandChoice> {
        Vec::new()
    }
}

pub trait ComponentHandler: Send + Sync {
    fn matches(&self, custom_id: &str) -> bool;

    fn execute(&self, ctx: &CommandContext) -> Result<()>;
}
