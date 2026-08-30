use crate::commands::ComponentHandler;
use crate::error::Result;
use crate::models::{AutocompleteResponse, Interaction, InteractionType};
use crate::rest::RestClient;

use super::{CommandContext, SlashCommand};

pub struct CommandRegistry {
    commands: Vec<Box<dyn SlashCommand>>,
    components: Vec<Box<dyn ComponentHandler>>,
}

impl CommandRegistry {
    pub fn new() -> Self {
        Self {
            commands: Vec::new(),
            components: Vec::new(),
        }
    }

    pub fn register(mut self, command: impl SlashCommand + 'static) -> Self {
        self.commands.push(Box::new(command));
        self
    }

    pub fn register_component(mut self, handler: impl ComponentHandler + 'static) -> Self {
        self.components.push(Box::new(handler));
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
            InteractionType::MessageComponent | InteractionType::ModalSubmit => {
                self.dispatch_component(rest, interaction)
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
        let Some(name) = &data.name else { return };
        let Some(command) = self.find(name) else {
            eprintln!("Order received but not recorded locally: {name}");
            return;
        };
        let ctx = CommandContext::new(rest, interaction);
        if let Err(e) = command.execute(&ctx) {
            eprintln!("Error executing command '{name}': {e}");
        }
    }

    fn dispatch_autocomplete(&self, rest: &RestClient, interaction: &Interaction) {
        let Some(data) = &interaction.data else {
            return;
        };
        let Some(name) = &data.name else { return };
        let Some(command) = self.find(name) else {
            return;
        };
        let ctx = CommandContext::new(rest, interaction);
        let response = AutocompleteResponse::new(command.autocomplete(&ctx));
        if let Err(e) =
            rest.create_interaction_response(&interaction.id, &interaction.token, &response)
        {
            eprintln!("Autocomplete response error: {e}");
        }
    }

    fn dispatch_component(&self, rest: &RestClient, interaction: &Interaction) {
        let Some(custom_id) = interaction
            .data
            .as_ref()
            .and_then(|d| d.custom_id.as_deref())
        else {
            return;
        };
        let Some(handler) = self.components.iter().find(|h| h.matches(custom_id)) else {
            eprintln!("No handler for the component:{custom_id}");
            return;
        };
        let ctx = CommandContext::new(rest, interaction);
        if let Err(e) = handler.execute(&ctx) {
            eprintln!("Execution error in component '{custom_id}': {e}");
        }
    }
}
