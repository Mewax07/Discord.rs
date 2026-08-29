use crate::error::Result;
use crate::models::{AutocompleteResponse, Interaction, InteractionType};
use crate::rest::RestClient;

use super::{CommandContext, SlashCommand};

pub struct CommandRegistry {
    commands: Vec<Box<dyn SlashCommand>>,
}

impl CommandRegistry {
    pub fn new() -> Self {
        Self {
            commands: Vec::new(),
        }
    }

    pub fn register(mut self, command: impl SlashCommand + 'static) -> Self {
        self.commands.push(Box::new(command));
        self
    }

    pub fn sync_with_discord(&self, rest: &RestClient, app_id: &str, guild_id: &str) -> Result<()> {
        let definitions: Vec<_> = self.commands.iter().map(|c| c.definition()).collect();
        rest.register_guild_commands(app_id, guild_id, &definitions)?;
        Ok(())
    }

    pub fn dispatch(&self, rest: &RestClient, interaction: &Interaction) {
        match interaction.kind {
            InteractionType::ApplicationCommand => self.dispatch_command(rest, interaction),
            InteractionType::ApplicationCommandAutocomplete => {
                self.dispatch_autocomplete(rest, interaction)
            }
            _ => {}
        }
    }

    fn find(&self, name: &str) -> Option<&dyn SlashCommand> {
        self.commands
            .iter()
            .find(|c| c.definition().name == name)
            .map(|c| c.as_ref())
    }

    fn dispatch_command(&self, rest: &RestClient, interaction: &Interaction) {
        let Some(data) = &interaction.data else {
            return;
        };
        let Some(command) = self.find(&data.name) else {
            eprintln!("Command received but not recorded locally: {}", data.name);
            return;
        };

        let ctx = CommandContext::new(rest, interaction);
        if let Err(e) = command.execute(&ctx) {
            eprintln!("Error executing the '{}' command: {e}", data.name);
        }
    }

    fn dispatch_autocomplete(&self, rest: &RestClient, interaction: &Interaction) {
        let Some(data) = &interaction.data else {
            return;
        };
        let Some(command) = self.find(&data.name) else {
            return;
        };

        let ctx = CommandContext::new(rest, interaction);
        let choices = command.autocomplete(&ctx);
        let response = AutocompleteResponse::new(choices);

        if let Err(e) =
            rest.create_interaction_response(&interaction.id, &interaction.token, &response)
        {
            eprintln!("Autocomplete response error: {e}");
        }
    }
}
