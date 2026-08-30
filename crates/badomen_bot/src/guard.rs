use discord::models::{Interaction, InteractionResponse, MessagePayload};
use discord::rest::RestClient;

use crate::logs;
use crate::ui;

pub struct HomeGuild {
    pub id: String,
}

impl HomeGuild {
    pub fn new(id: impl Into<String>) -> Self {
        Self { id: id.into() }
    }

    pub fn owns(&self, guild_id: Option<&str>) -> bool {
        guild_id == Some(self.id.as_str())
    }

    pub fn accepts(&self, interaction: &Interaction) -> bool {
        self.owns(interaction.guild_id.as_deref())
    }

    pub fn refuse(&self, rest: &RestClient, interaction: &Interaction) {
        let where_from = interaction
            .guild_id
            .as_deref()
            .map(|id| format!("guild {id}"))
            .unwrap_or_else(|| "a direct message".to_string());

        logs::warn(
            "guard",
            format!(
                "refused {} from {where_from}",
                interaction
                    .data
                    .as_ref()
                    .and_then(|data| data.name.clone())
                    .or_else(|| interaction
                        .data
                        .as_ref()
                        .and_then(|data| data.custom_id.clone()))
                    .unwrap_or_else(|| "interaction".to_string())
            ),
        );

        let response = InteractionResponse::message(
            MessagePayload::widget(ui::fail(
                "Unavailable here",
                "This bot only serves its home server. Nothing it offers can be used from anywhere else.",
            ))
            .ephemeral(),
        );

        if let Err(e) =
            rest.create_interaction_response(&interaction.id, &interaction.token, &response)
        {
            logs::debug("guard", format!("refusal not delivered: {e}"));
        }
    }

    pub fn enforce_membership(&self, rest: &RestClient, guild_id: &str, name: Option<&str>) {
        if guild_id == self.id {
            return;
        }

        logs::warn(
            "guard",
            format!(
                "leaving unauthorised guild {guild_id} ({})",
                name.unwrap_or("unnamed")
            ),
        );

        match rest.leave_guild(guild_id) {
            Ok(()) => logs::info("guard", format!("left guild {guild_id}")),
            Err(e) => logs::error("guard", format!("could not leave {guild_id}: {e}")),
        }
    }
}
