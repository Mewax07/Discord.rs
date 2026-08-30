use serde_json::Value;

use crate::error::Result;
use crate::models::{
    ActionRow, Channel, Component, Interaction, InteractionData, InteractionDataOption,
    InteractionResponse, Message, MessagePayload, ModalResponse, NewChannel, PermissionOverwrite,
    Role, User, PERM_ADMINISTRATOR, PERM_MANAGE_GUILD,
};
use crate::rest::RestClient;

const EMPTY_OPTIONS: &[InteractionDataOption] = &[];
const EMPTY_ROLES: &[String] = &[];

pub struct CommandContext<'a> {
    rest: &'a RestClient,
    interaction: &'a Interaction,
}

impl<'a> CommandContext<'a> {
    pub fn new(rest: &'a RestClient, interaction: &'a Interaction) -> Self {
        Self { rest, interaction }
    }

    pub fn rest(&self) -> &'a RestClient {
        self.rest
    }

    pub fn interaction(&self) -> &'a Interaction {
        self.interaction
    }

    pub fn data(&self) -> Option<&'a InteractionData> {
        self.interaction.data.as_ref()
    }

    pub fn author(&self) -> Option<&'a User> {
        self.interaction.author()
    }

    pub fn member_roles(&self) -> &'a [String] {
        self.interaction
            .member
            .as_ref()
            .map(|m| m.roles.as_slice())
            .unwrap_or(EMPTY_ROLES)
    }

    pub fn permission_bits(&self) -> u64 {
        self.interaction
            .member
            .as_ref()
            .map(|m| m.permission_bits())
            .unwrap_or(0)
    }

    pub fn has_permission(&self, bits: u64) -> bool {
        let held = self.permission_bits();
        held & PERM_ADMINISTRATOR != 0 || held & bits == bits
    }

    pub fn is_admin(&self) -> bool {
        self.has_permission(PERM_MANAGE_GUILD)
    }

    pub fn has_role(&self, role_id: &str) -> bool {
        self.member_roles().iter().any(|r| r == role_id)
    }

    pub fn guild_id(&self) -> Option<&'a str> {
        self.interaction.guild_id.as_deref()
    }

    pub fn channel_id(&self) -> Option<&'a str> {
        self.interaction.channel_id.as_deref()
    }

    pub fn message(&self) -> Option<&'a Message> {
        self.interaction.message.as_ref()
    }

    pub fn message_id(&self) -> Option<&'a str> {
        self.message().map(|m| m.id.as_str())
    }

    pub fn custom_id(&self) -> Option<&'a str> {
        self.data()?.custom_id.as_deref()
    }

    pub fn custom_id_parts(&self) -> Vec<&'a str> {
        self.custom_id()
            .map(|id| id.split('|').collect())
            .unwrap_or_default()
    }

    fn command_path(&self) -> (Vec<&'a str>, &'a [InteractionDataOption]) {
        let Some(data) = self.data() else {
            return (Vec::new(), EMPTY_OPTIONS);
        };

        let mut path = Vec::new();
        let mut scope: &'a [InteractionDataOption] = &data.options;

        while let Some(first) = scope.first() {
            if !first.value.is_null() {
                break;
            }
            path.push(first.name.as_str());
            scope = &first.options;
        }

        (path, scope)
    }

    pub fn subcommand(&self) -> Option<&'a str> {
        self.command_path().0.last().copied()
    }

    pub fn subcommand_group(&self) -> Option<&'a str> {
        let path = self.command_path().0;
        (path.len() > 1).then(|| path[0])
    }

    pub fn route(&self) -> (Option<&'a str>, Option<&'a str>) {
        (self.subcommand_group(), self.subcommand())
    }

    fn options_scope(&self) -> &'a [InteractionDataOption] {
        self.command_path().1
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

    pub fn modal_text(&self, custom_id: &str) -> Option<&'a str> {
        let value = self.modal_value(custom_id)?.trim();
        (!value.is_empty()).then_some(value)
    }

    pub fn respond(&self, response: InteractionResponse) -> Result<()> {
        self.rest.create_interaction_response(
            &self.interaction.id,
            &self.interaction.token,
            &response,
        )
    }

    pub fn reply(&self, payload: MessagePayload) -> Result<()> {
        self.respond(InteractionResponse::message(payload))
    }

    pub fn reply_text(&self, content: impl Into<String>) -> Result<()> {
        self.reply(MessagePayload::text(content))
    }

    pub fn reply_hidden(&self, content: impl Into<String>) -> Result<()> {
        self.reply(MessagePayload::text(content).ephemeral())
    }

    pub fn reply_widget(&self, components: Vec<Component>) -> Result<()> {
        self.reply(MessagePayload::widget(components))
    }

    pub fn reply_widget_hidden(&self, components: Vec<Component>) -> Result<()> {
        self.reply(MessagePayload::widget(components).ephemeral())
    }

    pub fn update(&self, payload: MessagePayload) -> Result<()> {
        self.respond(InteractionResponse::update(payload))
    }

    pub fn update_widget(&self, components: Vec<Component>) -> Result<()> {
        self.update(MessagePayload::widget(components))
    }

    pub fn update_widget_hidden(&self, components: Vec<Component>) -> Result<()> {
        self.update(MessagePayload::widget(components).ephemeral())
    }

    pub fn defer_update(&self) -> Result<()> {
        self.respond(InteractionResponse::deferred_update())
    }

    pub fn show_modal(
        &self,
        custom_id: impl Into<String>,
        title: impl Into<String>,
        rows: Vec<ActionRow>,
    ) -> Result<()> {
        let response = ModalResponse::new(custom_id, title, rows);
        self.rest.create_interaction_response(
            &self.interaction.id,
            &self.interaction.token,
            &response,
        )
    }

    pub fn send(&self, channel_id: &str, payload: MessagePayload) -> Result<Message> {
        self.rest.create_message(channel_id, &payload)
    }

    pub fn send_file(
        &self,
        channel_id: &str,
        payload: MessagePayload,
        file_name: &str,
        file_bytes: &[u8],
    ) -> Result<Message> {
        self.rest
            .create_message_with_file(channel_id, &payload, file_name, file_bytes)
    }

    pub fn edit(
        &self,
        channel_id: &str,
        message_id: &str,
        payload: MessagePayload,
    ) -> Result<Message> {
        self.rest.edit_message(channel_id, message_id, &payload)
    }

    pub fn direct_message(&self, user_id: &str, payload: MessagePayload) -> Result<Message> {
        self.rest.send_direct_message(user_id, &payload)
    }

    pub fn create_channel(&self, guild_id: &str, channel: NewChannel) -> Result<Channel> {
        self.rest.create_channel(guild_id, &channel)
    }

    pub fn delete_channel(&self, channel_id: &str) -> Result<()> {
        self.rest.delete_channel(channel_id)
    }

    pub fn set_channel_permission(
        &self,
        channel_id: &str,
        overwrite: &PermissionOverwrite,
    ) -> Result<()> {
        let allow = overwrite
            .allow
            .as_deref()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(0);
        let deny = overwrite
            .deny
            .as_deref()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(0);
        self.rest
            .set_channel_permission(channel_id, &overwrite.id, overwrite.kind, allow, deny)
    }

    pub fn clear_channel_permission(&self, channel_id: &str, target_id: &str) -> Result<()> {
        self.rest.delete_channel_permission(channel_id, target_id)
    }

    pub fn fetch_all_messages(&self, channel_id: &str) -> Result<Vec<Message>> {
        self.rest.fetch_all_messages(channel_id)
    }

    pub fn recent_messages(&self, channel_id: &str, limit: u16) -> Result<Vec<Message>> {
        self.rest
            .get_channel_messages_limited(channel_id, None, limit)
    }

    pub fn bulk_delete(&self, channel_id: &str, message_ids: &[String]) -> Result<()> {
        self.rest.bulk_delete_messages(channel_id, message_ids)
    }

    pub fn delete_message(&self, channel_id: &str, message_id: &str) -> Result<()> {
        self.rest.delete_message(channel_id, message_id)
    }

    pub fn expire_poll(&self, channel_id: &str, message_id: &str) -> Result<Message> {
        self.rest.expire_poll(channel_id, message_id)
    }

    pub fn add_role(&self, guild_id: &str, user_id: &str, role_id: &str) -> Result<()> {
        self.rest.add_member_role(guild_id, user_id, role_id)
    }

    pub fn remove_role(&self, guild_id: &str, user_id: &str, role_id: &str) -> Result<()> {
        self.rest.remove_member_role(guild_id, user_id, role_id)
    }
}
