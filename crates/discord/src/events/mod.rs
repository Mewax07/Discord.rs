mod payload;

pub use payload::parse_dispatch;

use crate::models::{Interaction, Message, User};

#[derive(Debug)]
pub enum Event {
    Ready,
    GuildCreate {
        id: String,
        name: Option<String>,
    },
    MessageCreate(Message),
    InteractionCreate(Interaction),
    GuildMemberAdd {
        guild_id: String,
        user: User,
    },
    GuildMemberRemove {
        guild_id: String,
        user: User,
    },
    /// more
    Unknown {
        name: String,
    },
}
