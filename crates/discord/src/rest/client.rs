use serde::Serialize;
use serde_json::Value;

use crate::{
    Result, models::{
        ActionRow, ApplicationInfo, Channel, CommandDefinition, Embed, PermissionOverwrite, RegisteredCommand,
    }, net::HttpClient,
};

const API: &str = "/api/v10";

pub struct RestClient {
    http: HttpClient,
    token: String,
}

#[derive(Serialize)]
struct CreateMessage<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<&'a str>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    embeds: Vec<Embed>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    components: Vec<ActionRow>,
}

#[derive(Serialize)]
struct CreateChannelBody {
    name: String,
    #[serde(rename = "type")]
    kind: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    parent_id: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    permission_overwrites: Vec<PermissionOverwrite>,
}

impl RestClient {
    pub fn new(token: impl Into<String>) -> Self {
        Self {
            http: HttpClient::new("discord.com"),
            token: token.into(),
        }
    }

    pub fn send_message(
        &self,
        channel_id: &str,
        content: Option<&str>,
        embeds: Vec<Embed>,
        components: Vec<ActionRow>,
    ) -> Result<Value> {
        let path = format!("{API}/channels/{channel_id}/messages");
        let body = CreateMessage {
            content,
            embeds,
            components,
        };
        self.http.post_json(&path, &self.token, &body)
    }

    pub fn get_application_info(&self) -> Result<ApplicationInfo> {
        let path = format!("{API}/oauth2/applications/@me");
        let value = self.http.get_json(&path, &self.token)?;
        Ok(serde_json::from_value(value)?)
    }

    pub fn register_global_command(
        &self,
        application_id: &str,
        command: &CommandDefinition,
    ) -> Result<RegisteredCommand> {
        let path = format!("{API}/applications/{application_id}/commands");
        let value = self.http.post_json(&path, &self.token, command)?;
        Ok(serde_json::from_value(value)?)
    }

    pub fn register_guild_command(
        &self,
        application_id: &str,
        guild_id: &str,
        command: &CommandDefinition,
    ) -> Result<RegisteredCommand> {
        let path = format!("{API}/applications/{application_id}/guilds/{guild_id}/commands");
        let value = self.http.post_json(&path, &self.token, command)?;
        Ok(serde_json::from_value(value)?)
    }

    pub fn register_guild_commands(
        &self,
        application_id: &str,
        guild_id: &str,
        commands: &[CommandDefinition],
    ) -> Result<Vec<RegisteredCommand>> {
        let path = format!("{API}/applications/{application_id}/guilds/{guild_id}/commands");
        let value = self.http.put_json(&path, &self.token, commands)?;
        Ok(serde_json::from_value(value)?)
    }

    pub fn create_interaction_response<T: Serialize + ?Sized>(
        &self,
        interaction_id: &str,
        interaction_token: &str,
        response: &T,
    ) -> Result<()> {
        let path = format!("{API}/interactions/{interaction_id}/{interaction_token}/callback");
        self.http.post_json(&path, &self.token, response)?;
        Ok(())
    }

    pub fn create_channel(
        &self,
        guild_id: &str,
        name: &str,
        parent_id: Option<&str>,
        overwrites: Vec<PermissionOverwrite>,
    ) -> Result<Channel> {
        let path = format!("{API}/guilds/{guild_id}/channels");
        let body = CreateChannelBody {
            name: name.to_string(),
            kind: 0,
            parent_id: parent_id.map(String::from),
            permission_overwrites: overwrites,
        };
        let value = self.http.post_json(&path, &self.token, &body)?;
        Ok(serde_json::from_value(value)?)
    }

    pub fn delete_channel(&self, channel_id: &str) -> Result<()> {
        let path = format!("{API}/channels/{channel_id}");
        self.http.delete(&path, &self.token)?;
        Ok(())
    }
}
