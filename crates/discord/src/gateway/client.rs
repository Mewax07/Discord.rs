use std::time::Instant;
use std::{thread, time::Duration};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{Error, Result};
use crate::gateway::presence::{build_presence, ActivityType, PresenceStatus};
use crate::net::ws::Message;
use crate::net::WebSocket;
use crate::shutdown;

use super::opcode as op;

pub struct GatewayConfig {
    pub token: String,
    pub intents: u32,
    pub os: &'static str,
    pub presence: Option<(String, ActivityType, PresenceStatus)>,
}

impl GatewayConfig {
    pub fn new(token: impl Into<String>, intents: u32) -> Self {
        Self {
            token: token.into(),
            intents,
            os: std::env::consts::OS,
            presence: None,
        }
    }

    pub fn with_presence(
        mut self,
        name: impl Into<String>,
        kind: ActivityType,
        status: PresenceStatus,
    ) -> Self {
        self.presence = Some((name.into(), kind, status));
        self
    }
}

#[derive(Debug)]
pub enum GatewayEvent {
    Dispatch { name: String, data: Value },
}

enum PumpOutcome {
    Reconnect,
    Shutdown,
}

#[derive(Serialize)]
struct IdentifyProperties {
    os: &'static str,
    browser: &'static str,
    device: &'static str,
}

#[derive(Serialize)]
struct IdentifyData<'a> {
    token: &'a str,
    intents: u32,
    properties: IdentifyProperties,
}

#[derive(Serialize)]
struct ResumeData<'a> {
    token: &'a str,
    session_id: &'a str,
    seq: u64,
}

#[derive(Deserialize)]
struct RawPayload {
    op: u8,
    #[serde(default)]
    d: Value,
    #[serde(default)]
    s: Option<u64>,
    #[serde(default)]
    t: Option<String>,
}

#[derive(Deserialize)]
struct HelloData {
    heartbeat_interval: u64,
}

#[derive(Deserialize)]
struct ReadyData {
    session_id: String,
    resume_gateway_url: String,
}

struct Session {
    id: String,
    resume_host: String,
    seq: u64,
}

pub struct Gateway {
    config: GatewayConfig,
    session: Option<Session>,
}

impl Gateway {
    pub fn new(config: GatewayConfig) -> Self {
        Self {
            config,
            session: None,
        }
    }

    pub fn run(&mut self, mut on_event: impl FnMut(GatewayEvent)) {
        let mut backoff = Duration::from_secs(1);

        loop {
            if shutdown::requested() {
                println!("gateway: Stop requested before reconnection, clean shutdown");
                return;
            }

            match self.connect_and_pump(&mut on_event) {
                Ok(PumpOutcome::Shutdown) => {
                    println!("gateway: Connection closed properly");
                    return;
                }
                Ok(PumpOutcome::Reconnect) => {
                    backoff = Duration::from_secs(1);
                }
                Err(e) => {
                    eprintln!("gateway: Disconnected ({e}), reconnection in {backoff:?}");
                    thread::sleep(backoff);
                    backoff = (backoff * 2).min(Duration::from_secs(60));
                }
            }
        }
    }

    fn connect_and_pump(&mut self, on_event: &mut impl FnMut(GatewayEvent)) -> Result<PumpOutcome> {
        let host = self
            .session
            .as_ref()
            .map(|s| s.resume_host.clone())
            .unwrap_or_else(|| "gateway.discord.gg".to_string());

        let mut ws = WebSocket::connect(&host, "/?v=10&encoding=json")?;

        let hello = match ws.read_message()? {
            Message::Text(text) => {
                let p: RawPayload = serde_json::from_str(&text)?;
                if p.op != op::HELLO {
                    return Err(Error::WebSocket(format!(
                        "Expected: HELLO, Received: op {}",
                        p.op
                    )));
                }
                serde_json::from_value::<HelloData>(p.d)?
            }
            other => {
                return Err(Error::WebSocket(format!(
                    "unexpected message before HELLO: {other:?}"
                )))
            }
        };
        let heartbeat_interval = Duration::from_millis(hello.heartbeat_interval);

        if let Some(session) = &self.session {
            let resume = ResumeData {
                token: &self.config.token,
                session_id: &session.id,
                seq: session.seq,
            };
            self.send(&mut ws, op::RESUME, &resume)?;
        } else {
            let identify = IdentifyData {
                token: &self.config.token,
                intents: self.config.intents,
                properties: IdentifyProperties {
                    os: self.config.os,
                    browser: "badomen",
                    device: "badomen",
                },
            };
            self.send(&mut ws, op::IDENTIFY, &identify)?;

            if let Some((name, kind, status)) = &self.config.presence {
                let presence = build_presence(name, *kind, *status);
                self.send(&mut ws, op::PRESENCE_UPDATE, &presence)?;
            }
        }

        let poll_interval = heartbeat_interval.min(Duration::from_millis(500));
        let mut last_heartbeat = Instant::now();
        let mut awaiting_ack = false;

        loop {
            if shutdown::requested() {
                let _ = ws.close();
                return Ok(PumpOutcome::Shutdown);
            }

            match ws.read_message_with_timeout(poll_interval)? {
                None => {
                    if last_heartbeat.elapsed() >= heartbeat_interval {
                        if awaiting_ack {
                            return Err(Error::WebSocket(
                                "Missing heartbeat ACK, zombie connection".into(),
                            ));
                        }
                        self.send_heartbeat(&mut ws)?;
                        awaiting_ack = true;
                        last_heartbeat = Instant::now();
                    }
                }
                Some(Message::Text(text)) => {
                    let payload: RawPayload = serde_json::from_str(&text)?;

                    if let (Some(seq), Some(session)) = (payload.s, self.session.as_mut()) {
                        session.seq = seq;
                    }

                    match payload.op {
                        op::DISPATCH => {
                            if payload.t.as_deref() == Some("READY") {
                                let ready: ReadyData = serde_json::from_value(payload.d.clone())?;
                                self.session = Some(Session {
                                    id: ready.session_id,
                                    resume_host: strip_scheme(&ready.resume_gateway_url),
                                    seq: payload.s.unwrap_or(0),
                                });
                            }
                            if let Some(name) = payload.t {
                                on_event(GatewayEvent::Dispatch {
                                    name,
                                    data: payload.d,
                                });
                            }
                        }
                        op::HEARTBEAT => {
                            self.send_heartbeat(&mut ws)?;
                        }
                        op::HEARTBEAT_ACK => {
                            awaiting_ack = false;
                        }
                        op::RECONNECT => {
                            return Ok(PumpOutcome::Reconnect);
                        }
                        op::INVALID_SESSION => {
                            let resumable = payload.d.as_bool().unwrap_or(false);
                            if !resumable {
                                self.session = None;
                            }
                            thread::sleep(Duration::from_millis(1500));
                            return Ok(PumpOutcome::Reconnect);
                        }
                        _ => {}
                    }
                }
                Some(Message::Close) => return Ok(PumpOutcome::Reconnect),
                Some(Message::Binary(_)) => {}
            }
        }
    }

    fn send_heartbeat(&self, ws: &mut WebSocket) -> Result<()> {
        let seq = self.session.as_ref().map(|s| s.seq);
        self.send(ws, op::HEARTBEAT, &seq)
    }

    fn send(&self, ws: &mut WebSocket, opcode: u8, data: &impl Serialize) -> Result<()> {
        #[derive(Serialize)]
        struct Frame<'a, T> {
            op: u8,
            d: &'a T,
        }
        let payload = serde_json::to_string(&Frame {
            op: opcode,
            d: data,
        })?;
        ws.send_text(&payload)
    }
}

fn strip_scheme(url: &str) -> String {
    url.trim_start_matches("wss://")
        .trim_start_matches("https://")
        .trim_end_matches('/')
        .to_string()
}
