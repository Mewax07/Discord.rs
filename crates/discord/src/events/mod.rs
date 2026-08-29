mod payload;

pub use payload::parse_dispatch;

use crate::models::{Interaction, Message};

#[derive(Debug)]
pub enum Event {
    Ready,
    MessageCreate(Message),
    InteractionCreate(Interaction),
    /// more
    Unknown {
        name: String,
    },
}
