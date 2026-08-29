use serde::{Serialize, Serializer};

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum ActivityType {
    Playing = 0,
    Streaming = 1,
    Listening = 2,
    Watching = 3,
    Custom = 4,
    Competing = 5,
}

impl Serialize for ActivityType {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u8(*self as u8)
    }
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PresenceStatus {
    Online,
    Idle,
    Dnd,
    Invisible,
}

#[derive(Serialize)]
struct Activity<'a> {
    name: &'a str,
    #[serde(rename = "type")]
    kind: ActivityType,
}

#[derive(Serialize)]
struct PresenceUpdateData<'a> {
    since: Option<u64>,
    activities: Vec<Activity<'a>>,
    status: PresenceStatus,
    afk: bool,
}

pub fn build_presence(
    name: &str,
    kind: ActivityType,
    status: PresenceStatus,
) -> impl Serialize + '_ {
    PresenceUpdateData {
        since: None,
        activities: vec![Activity { name, kind }],
        status,
        afk: false,
    }
}
