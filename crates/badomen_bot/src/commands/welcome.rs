use std::sync::Arc;

use discord::models::{AllowedMentions, MessagePayload, User};
use discord::rest::RestClient;

use crate::logs;
use crate::storage::ConfigStore;
use crate::ui::{self, Theme};

const DEFAULT_JOIN_MESSAGE: &str = "Bienvenue {user} sur le serveur !";
const DEFAULT_LEAVE_MESSAGE: &str = "**{name}** a quitte le serveur.";

#[derive(Clone)]
pub struct WelcomeService {
    pub config: Arc<ConfigStore>,
    pub rest: Arc<RestClient>,
}

impl WelcomeService {
    pub fn announce_join(&self, guild_id: &str, user: &User) {
        let cfg = self.config.get(guild_id);
        let Some(channel_id) = cfg.welcome_channel_id.clone() else {
            return;
        };

        let theme = Theme::from_brand(&cfg.brand);
        let text = render(cfg.welcome_message.as_deref().unwrap_or(DEFAULT_JOIN_MESSAGE), user);
        let payload = MessagePayload::widget(ui::panel(theme.accent, vec![ui::text(text)]))
            .mentions(AllowedMentions::users(vec![user.id.clone()]));

        if let Err(e) = self.rest.create_message(&channel_id, &payload) {
            logs::error("welcome", format!("join announcement failed: {e}"));
        }
    }

    pub fn announce_leave(&self, guild_id: &str, user: &User) {
        let cfg = self.config.get(guild_id);
        let Some(channel_id) = cfg.leave_channel_id.clone() else {
            return;
        };

        let theme = Theme::from_brand(&cfg.brand);
        let text = render(cfg.leave_message.as_deref().unwrap_or(DEFAULT_LEAVE_MESSAGE), user);
        let payload =
            MessagePayload::widget(ui::panel(theme.accent, vec![ui::text(text)])).no_mentions();

        if let Err(e) = self.rest.create_message(&channel_id, &payload) {
            logs::error("welcome", format!("leave announcement failed: {e}"));
        }
    }
}

fn render(template: &str, user: &User) -> String {
    template
        .replace("{user}", &user.mention())
        .replace("{name}", user.display_name())
}
