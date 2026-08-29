use crate::error::Result;
use crate::models::{
    Channel, Embed, Interaction, InteractionData, InteractionDataOption, InteractionResponse, Role,
    User,
};
use crate::rest::RestClient;

pub struct CommandContext<'a> {
    rest: &'a RestClient,
    interaction: &'a Interaction,
}

impl<'a> CommandContext<'a> {
    pub fn new(rest: &'a RestClient, interaction: &'a Interaction) -> Self {
        Self { rest, interaction }
    }

    pub fn data(&self) -> Option<&InteractionData> {
        self.interaction.data.as_ref()
    }

    pub fn author(&self) -> Option<&User> {
        self.interaction.author()
    }

    pub fn guild_id(&self) -> Option<&str> {
        self.interaction.guild_id.as_deref()
    }

    pub fn channel_id(&self) -> Option<&str> {
        self.interaction.channel_id.as_deref()
    }

    fn raw_option(&self, name: &str) -> Option<&serde_json::Value> {
        self.data()?
            .options
            .iter()
            .find(|o| o.name == name)
            .map(|o| &o.value)
    }

    pub fn option_string(&self, name: &str) -> Option<&str> {
        self.raw_option(name)?.as_str()
    }

    pub fn option_integer(&self, name: &str) -> Option<i64> {
        self.raw_option(name)?.as_i64()
    }

    pub fn option_number(&self, name: &str) -> Option<f64> {
        self.raw_option(name)?.as_f64()
    }

    pub fn option_boolean(&self, name: &str) -> Option<bool> {
        self.raw_option(name)?.as_bool()
    }

    pub fn option_user(&self, name: &str) -> Option<&User> {
        let id = self.raw_option(name)?.as_str()?;
        self.data()?.resolved.users.get(id)
    }

    pub fn option_channel(&self, name: &str) -> Option<&Channel> {
        let id = self.raw_option(name)?.as_str()?;
        self.data()?.resolved.channels.get(id)
    }

    pub fn option_role(&self, name: &str) -> Option<&Role> {
        let id = self.raw_option(name)?.as_str()?;
        self.data()?.resolved.roles.get(id)
    }

    pub fn focused_option(&self) -> Option<&InteractionDataOption> {
        self.data()?.options.iter().find(|o| o.focused)
    }

    pub fn reply(&self, content: impl Into<String>) -> Result<()> {
        let response = InteractionResponse::message(content);
        self.rest.create_interaction_response(
            &self.interaction.id,
            &self.interaction.token,
            &response,
        )
    }

    pub fn reply_embed(&self, embed: Embed) -> Result<()> {
        let response = InteractionResponse::embed(embed);
        self.rest.create_interaction_response(
            &self.interaction.id,
            &self.interaction.token,
            &response,
        )
    }
}
