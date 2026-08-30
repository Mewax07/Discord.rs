use serde::Serialize;
use serde_json::Value;

use crate::{
    models::{
        ApplicationInfo, Channel, CommandDefinition, Member, Message, MessagePayload, NewChannel,
        RegisteredCommand, Role,
    },
    net::HttpClient,
    Result,
};

const API: &str = "/api/v10";

pub struct RestClient {
    http: HttpClient,
    token: String,
}

#[derive(Serialize)]
struct CreateDmBody<'a> {
    recipient_id: &'a str,
}

#[derive(Serialize)]
struct ChannelPermissionBody {
    #[serde(rename = "type")]
    kind: u8,
    allow: String,
    deny: String,
}

impl RestClient {
    pub fn new(token: impl Into<String>) -> Self {
        Self {
            http: HttpClient::new("discord.com"),
            token: token.into(),
        }
    }

    pub fn get_application_info(&self) -> Result<ApplicationInfo> {
        let path = format!("{API}/oauth2/applications/@me");
        let value = self.http.get_json(&path, &self.token)?;
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

    pub fn register_global_commands(
        &self,
        application_id: &str,
        commands: &[CommandDefinition],
    ) -> Result<Vec<RegisteredCommand>> {
        let path = format!("{API}/applications/{application_id}/commands");
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

    pub fn edit_interaction_response(
        &self,
        application_id: &str,
        interaction_token: &str,
        payload: &MessagePayload,
    ) -> Result<()> {
        let path =
            format!("{API}/webhooks/{application_id}/{interaction_token}/messages/@original");
        self.http.patch_json(&path, &self.token, payload)?;
        Ok(())
    }

    pub fn create_message(&self, channel_id: &str, payload: &MessagePayload) -> Result<Message> {
        let path = format!("{API}/channels/{channel_id}/messages");
        let value = self.http.post_json(&path, &self.token, payload)?;
        Ok(serde_json::from_value(value)?)
    }

    pub fn edit_message(
        &self,
        channel_id: &str,
        message_id: &str,
        payload: &MessagePayload,
    ) -> Result<Message> {
        let path = format!("{API}/channels/{channel_id}/messages/{message_id}");
        let value = self.http.patch_json(&path, &self.token, payload)?;
        Ok(serde_json::from_value(value)?)
    }

    pub fn delete_message(&self, channel_id: &str, message_id: &str) -> Result<()> {
        let path = format!("{API}/channels/{channel_id}/messages/{message_id}");
        self.http.delete(&path, &self.token)?;
        Ok(())
    }

    pub fn create_message_with_file(
        &self,
        channel_id: &str,
        payload: &MessagePayload,
        file_name: &str,
        file_bytes: &[u8],
    ) -> Result<Message> {
        let path = format!("{API}/channels/{channel_id}/messages");
        let payload_json = serde_json::to_string(payload)?;
        let value =
            self.http
                .post_multipart(&path, &self.token, &payload_json, file_name, file_bytes)?;
        Ok(serde_json::from_value(value)?)
    }

    pub fn get_message(&self, channel_id: &str, message_id: &str) -> Result<Message> {
        let path = format!("{API}/channels/{channel_id}/messages/{message_id}");
        let value = self.http.get_json(&path, &self.token)?;
        Ok(serde_json::from_value(value)?)
    }

    pub fn get_channel_messages(
        &self,
        channel_id: &str,
        before: Option<&str>,
    ) -> Result<Vec<Message>> {
        self.get_channel_messages_limited(channel_id, before, 100)
    }

    pub fn get_channel_messages_limited(
        &self,
        channel_id: &str,
        before: Option<&str>,
        limit: u16,
    ) -> Result<Vec<Message>> {
        let limit = limit.clamp(1, 100);
        let mut path = format!("{API}/channels/{channel_id}/messages?limit={limit}");
        if let Some(before) = before {
            path.push_str(&format!("&before={before}"));
        }
        let value = self.http.get_json(&path, &self.token)?;
        Ok(serde_json::from_value(value)?)
    }

    pub fn bulk_delete_messages(&self, channel_id: &str, message_ids: &[String]) -> Result<()> {
        let path = format!("{API}/channels/{channel_id}/messages/bulk-delete");
        let body = serde_json::json!({ "messages": message_ids });
        self.http.post_json(&path, &self.token, &body)?;
        Ok(())
    }

    pub fn expire_poll(&self, channel_id: &str, message_id: &str) -> Result<Message> {
        let path = format!("{API}/channels/{channel_id}/polls/{message_id}/expire");
        let value = self
            .http
            .post_json(&path, &self.token, &Value::Object(Default::default()))?;
        Ok(serde_json::from_value(value)?)
    }

    pub fn fetch_all_messages(&self, channel_id: &str) -> Result<Vec<Message>> {
        let mut all = Vec::new();
        let mut before: Option<String> = None;

        for _ in 0..20 {
            let batch = self.get_channel_messages(channel_id, before.as_deref())?;
            if batch.is_empty() {
                break;
            }
            let batch_len = batch.len();
            before = batch.last().map(|m| m.id.clone());
            all.extend(batch);
            if batch_len < 100 {
                break;
            }
        }

        all.reverse();
        Ok(all)
    }

    pub fn create_channel(&self, guild_id: &str, channel: &NewChannel) -> Result<Channel> {
        let path = format!("{API}/guilds/{guild_id}/channels");
        let value = self.http.post_json(&path, &self.token, channel)?;
        Ok(serde_json::from_value(value)?)
    }

    pub fn delete_channel(&self, channel_id: &str) -> Result<()> {
        let path = format!("{API}/channels/{channel_id}");
        self.http.delete(&path, &self.token)?;
        Ok(())
    }

    pub fn set_channel_permission(
        &self,
        channel_id: &str,
        target_id: &str,
        kind: u8,
        allow: u64,
        deny: u64,
    ) -> Result<()> {
        let path = format!("{API}/channels/{channel_id}/permissions/{target_id}");
        let body = ChannelPermissionBody {
            kind,
            allow: allow.to_string(),
            deny: deny.to_string(),
        };
        self.http.put_json(&path, &self.token, &body)?;
        Ok(())
    }

    pub fn delete_channel_permission(&self, channel_id: &str, target_id: &str) -> Result<()> {
        let path = format!("{API}/channels/{channel_id}/permissions/{target_id}");
        self.http.delete(&path, &self.token)?;
        Ok(())
    }

    pub fn create_dm_channel(&self, user_id: &str) -> Result<Channel> {
        let path = format!("{API}/users/@me/channels");
        let body = CreateDmBody {
            recipient_id: user_id,
        };
        let value = self.http.post_json(&path, &self.token, &body)?;
        Ok(serde_json::from_value(value)?)
    }

    pub fn send_direct_message(&self, user_id: &str, payload: &MessagePayload) -> Result<Message> {
        let channel = self.create_dm_channel(user_id)?;
        self.create_message(&channel.id, payload)
    }

    pub fn get_guild_roles(&self, guild_id: &str) -> Result<Vec<Role>> {
        let path = format!("{API}/guilds/{guild_id}/roles");
        let value = self.http.get_json(&path, &self.token)?;
        Ok(serde_json::from_value(value)?)
    }

    pub fn get_guild_member(&self, guild_id: &str, user_id: &str) -> Result<Member> {
        let path = format!("{API}/guilds/{guild_id}/members/{user_id}");
        let value = self.http.get_json(&path, &self.token)?;
        Ok(serde_json::from_value(value)?)
    }

    pub fn leave_guild(&self, guild_id: &str) -> Result<()> {
        let path = format!("{API}/users/@me/guilds/{guild_id}");
        self.http.delete(&path, &self.token)?;
        Ok(())
    }

    pub fn add_member_role(&self, guild_id: &str, user_id: &str, role_id: &str) -> Result<()> {
        let path = format!("{API}/guilds/{guild_id}/members/{user_id}/roles/{role_id}");
        self.http
            .put_json(&path, &self.token, &Value::Object(Default::default()))?;
        Ok(())
    }

    pub fn remove_member_role(&self, guild_id: &str, user_id: &str, role_id: &str) -> Result<()> {
        let path = format!("{API}/guilds/{guild_id}/members/{user_id}/roles/{role_id}");
        self.http.delete(&path, &self.token)?;
        Ok(())
    }
}
