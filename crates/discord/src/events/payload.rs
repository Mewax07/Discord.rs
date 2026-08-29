use serde_json::{from_value, Value};

use crate::{events::Event, Result};

pub fn parse_dispatch(name: &str, data: Value) -> Result<Event> {
    Ok(match name {
        "READY" => Event::Ready,
        "MESSAGE_CREATE" => Event::MessageCreate(from_value(data)?),
        "INTERACTION_CREATE" => Event::InteractionCreate(serde_json::from_value(data)?),
        other => Event::Unknown {
            name: other.to_string(),
        },
    })
}
