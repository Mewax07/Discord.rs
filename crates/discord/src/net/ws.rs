use std::{
    io::{self, Read, Write},
    time::Duration,
};

use super::tls::TlsStream;
use crate::error::{Error, Result};

pub struct WebSocket {
    stream: TlsStream,
}

#[derive(Debug)]
pub enum Message {
    Text(String),
    Binary(Vec<u8>),
    Close,
}

const OP_CONTINUATION: u8 = 0x0;
const OP_TEXT: u8 = 0x1;
const OP_BINARY: u8 = 0x2;
const OP_CLOSE: u8 = 0x8;
const OP_PING: u8 = 0x9;
const OP_PONG: u8 = 0xA;

const MAX_FRAME: u64 = 16 * 1024 * 1024;

impl WebSocket {
    pub fn connect(host: &str, path: &str) -> Result<Self> {
        let mut stream = TlsStream::connect(host, 443)?;

        let key = generate_ws_key();
        let req = format!(
            "GET {path} HTTP/1.1\r\n\
             Host: {host}\r\n\
             Upgrade: websocket\r\n\
             Connection: Upgrade\r\n\
             Sec-WebSocket-Key: {key}\r\n\
             Sec-WebSocket-Version: 13\r\n\
             \r\n"
        );
        stream.write_all(req.as_bytes())?;
        stream.flush()?;

        let mut buf = Vec::new();
        let mut byte = [0u8; 1];
        loop {
            stream.read_exact(&mut byte)?;
            buf.push(byte[0]);
            if buf.ends_with(b"\r\n\r\n") {
                break;
            }
            if buf.len() > 8192 {
                return Err(Error::WebSocket("Upgrade headers that are too long".into()));
            }
        }

        let head = String::from_utf8_lossy(&buf);
        if !head.starts_with("HTTP/1.1 101") {
            return Err(Error::WebSocket(format!("Upgrade Denied: {head}")));
        }

        Ok(Self { stream })
    }

    pub fn send_text(&mut self, text: &str) -> Result<()> {
        self.write_frame(OP_TEXT, text.as_bytes())
    }

    pub fn send_pong(&mut self, payload: &[u8]) -> Result<()> {
        self.write_frame(OP_PONG, payload)
    }

    pub fn close(&mut self) -> Result<()> {
        self.write_frame(OP_CLOSE, &[])
    }

    fn write_frame(&mut self, opcode: u8, payload: &[u8]) -> Result<()> {
        let mut frame = Vec::with_capacity(payload.len() + 14);
        frame.push(0x80 | opcode);

        let masking_key = random_mask();
        let len = payload.len();

        if len <= 125 {
            frame.push(0x80 | len as u8);
        } else if len <= 65535 {
            frame.push(0x80 | 126);
            frame.extend_from_slice(&(len as u16).to_be_bytes());
        } else {
            frame.push(0x80 | 127);
            frame.extend_from_slice(&(len as u64).to_be_bytes());
        }

        frame.extend_from_slice(&masking_key);

        let mut masked_payload = payload.to_vec();
        for (i, b) in masked_payload.iter_mut().enumerate() {
            *b ^= masking_key[i % 4];
        }
        frame.extend_from_slice(&masked_payload);

        self.stream.write_all(&frame)?;
        self.stream.flush()?;
        Ok(())
    }

    pub fn read_message(&mut self) -> Result<Message> {
        self.stream.set_read_timeout(None)?;
        let mut first = [0u8; 1];
        self.stream.read_exact(&mut first)?;
        self.assemble_message(first[0])
    }

    pub fn read_message_with_timeout(&mut self, timeout: Duration) -> Result<Option<Message>> {
        self.stream.set_read_timeout(Some(timeout))?;

        let mut first = [0u8; 1];
        match self.stream.read(&mut first) {
            Ok(0) => return Err(Error::WebSocket("Connection closed by the peer".into())),
            Ok(_) => {}
            Err(e) if is_timeout(&e) => return Ok(None),
            Err(e) => return Err(e.into()),
        }

        self.stream.set_read_timeout(None)?;
        self.assemble_message(first[0]).map(Some)
    }

    fn assemble_message(&mut self, first_header_byte: u8) -> Result<Message> {
        let mut assembled: Vec<u8> = Vec::new();
        let mut assembled_opcode: Option<u8> = None;
        let mut pending_first_byte = Some(first_header_byte);

        loop {
            let (fin, opcode, payload) = self.read_frame(pending_first_byte.take())?;

            match opcode {
                OP_PING => {
                    self.send_pong(&payload)?;
                    continue;
                }
                OP_PONG => continue,
                OP_CLOSE => return Ok(Message::Close),
                OP_CONTINUATION => assembled.extend_from_slice(&payload),
                OP_TEXT | OP_BINARY => {
                    assembled_opcode = Some(opcode);
                    assembled.extend_from_slice(&payload);
                }
                _ => return Err(Error::WebSocket(format!("unknown opcode: {opcode}"))),
            }

            if fin {
                let final_opcode = assembled_opcode
                    .ok_or(Error::WebSocket("continuation frame with no start".into()))?;
                return match final_opcode {
                    OP_TEXT => String::from_utf8(assembled)
                        .map(Message::Text)
                        .map_err(|_| Error::WebSocket("non-UTF8 text".into())),
                    OP_BINARY => Ok(Message::Binary(assembled)),
                    _ => unreachable!(),
                };
            }
        }
    }

    fn read_frame(&mut self, first_byte: Option<u8>) -> Result<(bool, u8, Vec<u8>)> {
        let b0 = match first_byte {
            Some(b) => b,
            None => {
                let mut b = [0u8; 1];
                self.stream.read_exact(&mut b)?;
                b[0]
            }
        };

        let mut b1 = [0u8; 1];
        self.stream.read_exact(&mut b1)?;

        let fin = b0 & 0x80 != 0;
        let opcode = b0 & 0x0F;
        let masked = b1[0] & 0x80 != 0;
        let mut len = (b1[0] & 0x7F) as u64;

        if len == 126 {
            let mut ext = [0u8; 2];
            self.stream.read_exact(&mut ext)?;
            len = u16::from_be_bytes(ext) as u64;
        } else if len == 127 {
            let mut ext = [0u8; 8];
            self.stream.read_exact(&mut ext)?;
            len = u64::from_be_bytes(ext);
        }

        let mask_key = if masked {
            let mut m = [0u8; 4];
            self.stream.read_exact(&mut m)?;
            Some(m)
        } else {
            None
        };

        if len > MAX_FRAME {
            return Err(Error::WebSocket("frame is too large".into()));
        }

        let mut payload = vec![0u8; len as usize];
        self.stream.read_exact(&mut payload)?;

        if let Some(key) = mask_key {
            for (i, b) in payload.iter_mut().enumerate() {
                *b ^= key[i % 4];
            }
        }

        Ok((fin, opcode, payload))
    }
}

fn is_timeout(e: &io::Error) -> bool {
    matches!(
        e.kind(),
        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
    )
}

fn random_mask() -> [u8; 4] {
    let mut buf = [0u8; 4];
    getrandom::fill(&mut buf).expect("getrandom failed");
    buf
}

fn generate_ws_key() -> String {
    let mut raw = [0u8; 16];
    getrandom::fill(&mut raw).expect("getrandom failed");
    base64_encode(&raw)
}

fn base64_encode(data: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((data.len() + 2) / 3 * 4);
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | (b[2] as u32);
        out.push(TABLE[((n >> 18) & 0x3F) as usize] as char);
        out.push(TABLE[((n >> 12) & 0x3F) as usize] as char);
        out.push(if chunk.len() > 1 {
            TABLE[((n >> 6) & 0x3F) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[(n & 0x3F) as usize] as char
        } else {
            '='
        });
    }
    out
}
