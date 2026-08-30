use serde_json::Value;

use crate::error::Result;
use crate::models::{
    Channel, Embed, Interaction, InteractionData, InteractionDataOption, InteractionResponse,
    ModalResponse, PermissionOverwrite, Role, User,
};
use crate::rest::RestClient;

const EMPTY_OPTIONS: &[InteractionDataOption] = &[];

pub struct CommandContext<'a> {
    rest: &'a RestClient,
    interaction: &'a Interaction,
}

impl<'a> CommandContext<'a> {
    pub fn new(rest: &'a RestClient, interaction: &'a Interaction) -> Self {
        Self { rest, interaction }
    }

    pub fn data(&self) -> Option<&'a InteractionData> {
        self.interaction.data.as_ref()
    }

    pub fn author(&self) -> Option<&'a User> {
        self.interaction.author()
    }

    pub fn guild_id(&self) -> Option<&'a str> {
        self.interaction.guild_id.as_deref()
    }

    pub fn channel_id(&self) -> Option<&'a str> {
        self.interaction.channel_id.as_deref()
    }

    pub fn custom_id(&self) -> Option<&'a str> {
        self.data()?.custom_id.as_deref()
    }

    pub fn subcommand(&self) -> Option<&'a str> {
        let first = self.data()?.options.first()?;
        first.value.is_null().then(|| first.name.as_str())
    }

    fn options_scope(&self) -> &'a [InteractionDataOption] {
        let Some(data) = self.data() else {
            return EMPTY_OPTIONS;
        };
        match data.options.first() {
            Some(first) if first.value.is_null() => &first.options,
            _ => &data.options,
        }
    }

    fn raw_option(&self, name: &str) -> Option<&'a Value> {
        self.options_scope()
            .iter()
            .find(|o| o.name == name)
            .map(|o| &o.value)
    }

    pub fn option_string(&self, name: &str) -> Option<&'a str> {
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

    pub fn option_user(&self, name: &str) -> Option<&'a User> {
        let id = self.raw_option(name)?.as_str()?;
        self.data()?.resolved.users.get(id)
    }

    pub fn option_channel(&self, name: &str) -> Option<&'a Channel> {
        let id = self.raw_option(name)?.as_str()?;
        self.data()?.resolved.channels.get(id)
    }

    pub fn option_role(&self, name: &str) -> Option<&'a Role> {
        let id = self.raw_option(name)?.as_str()?;
        self.data()?.resolved.roles.get(id)
    }

    pub fn focused_option(&self) -> Option<&'a InteractionDataOption> {
        self.options_scope().iter().find(|o| o.focused)
    }

    pub fn selected_values(&self) -> &'a [String] {
        self.data().map(|d| d.values.as_slice()).unwrap_or(&[])
    }

    pub fn selected_value(&self) -> Option<&'a str> {
        self.selected_values().first().map(String::as_str)
    }

    pub fn modal_value(&self, custom_id: &str) -> Option<&'a str> {
        self.data()?.modal_value(custom_id)
    }

    pub fn reply(&self, content: impl Into<String>) -> Result<()> {
        self.reply_response(InteractionResponse::message(content))
    }

    pub fn reply_embed(&self, embed: Embed) -> Result<()> {
        self.reply_response(InteractionResponse::embed(embed))
    }

    pub fn reply_response(&self, response: InteractionResponse) -> Result<()> {
        self.rest.create_interaction_response(
            &self.interaction.id,
            &self.interaction.token,
            &response,
        )
    }

    pub fn show_modal(
        &self,
        custom_id: impl Into<String>,
        title: impl Into<String>,
        rows: Vec<crate::models::ActionRow>,
    ) -> Result<()> {
        let response = ModalResponse::new(custom_id, title, rows);
        self.rest.create_interaction_response(
            &self.interaction.id,
            &self.interaction.token,
            &response,
        )
    }

    pub fn create_channel(
        &self,
        guild_id: &str,
        name: &str,
        parent_id: Option<&str>,
        overwrites: Vec<PermissionOverwrite>,
    ) -> Result<Channel> {
        self.rest
            .create_channel(guild_id, name, parent_id, overwrites)
    }

    pub fn delete_channel(&self, channel_id: &str) -> Result<()> {
        self.rest.delete_channel(channel_id)
    }

    pub fn send_channel_message(
        &self,
        channel_id: &str,
        content: Option<&str>,
        embeds: Vec<Embed>,
        components: Vec<crate::models::ActionRow>,
    ) -> Result<()> {
        self.rest
            .send_message(channel_id, content, embeds, components)?;
        Ok(())
    }
}
