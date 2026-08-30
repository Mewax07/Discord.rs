use std::fs;
use std::path::{Path, PathBuf};

use ring::digest;
use ring::rand::SystemRandom;
use ring::signature::{Ed25519KeyPair, KeyPair, UnparsedPublicKey, ED25519};

const B64: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

pub fn base64url_encode(input: &[u8]) -> String {
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);

    for chunk in input.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let bits = (b0 << 16) | (b1 << 8) | b2;

        out.push(B64[(bits >> 18) as usize & 63] as char);
        out.push(B64[(bits >> 12) as usize & 63] as char);
        if chunk.len() > 1 {
            out.push(B64[(bits >> 6) as usize & 63] as char);
        }
        if chunk.len() > 2 {
            out.push(B64[bits as usize & 63] as char);
        }
    }

    out
}

pub fn base64url_decode(input: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(input.len() * 3 / 4);
    let mut bits: u32 = 0;
    let mut held = 0u32;

    for byte in input.bytes() {
        let value = B64.iter().position(|c| *c == byte)? as u32;
        bits = (bits << 6) | value;
        held += 6;
        if held >= 8 {
            held -= 8;
            out.push((bits >> held) as u8);
        }
    }

    Some(out)
}

pub fn hex_encode(input: &[u8]) -> String {
    let mut out = String::with_capacity(input.len() * 2);
    for byte in input {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

pub fn hex_decode(input: &str) -> Option<Vec<u8>> {
    let cleaned = input.trim();
    if cleaned.len() % 2 != 0 {
        return None;
    }

    let bytes = cleaned.as_bytes();
    let mut out = Vec::with_capacity(cleaned.len() / 2);

    for pair in bytes.chunks(2) {
        let text = std::str::from_utf8(pair).ok()?;
        out.push(u8::from_str_radix(text, 16).ok()?);
    }

    Some(out)
}

pub fn sha256_hex(input: &str) -> String {
    hex_encode(digest::digest(&digest::SHA256, input.as_bytes()).as_ref())
}

pub fn constant_time_eq(left: &str, right: &str) -> bool {
    let (left, right) = (left.as_bytes(), right.as_bytes());
    if left.len() != right.len() {
        return false;
    }
    let mut diff = 0u8;
    for (a, b) in left.iter().zip(right.iter()) {
        diff |= a ^ b;
    }
    diff == 0
}

pub struct Signer {
    keypair: Ed25519KeyPair,
    public_key: Vec<u8>,
}

impl Signer {
    pub fn open(path: impl Into<PathBuf>) -> std::io::Result<Self> {
        let path: PathBuf = path.into();

        let pkcs8 = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(_) => {
                let generated = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new())
                    .map_err(|_| invalid("unable to generate a signing key"))?;
                write_private(&path, generated.as_ref())?;
                generated.as_ref().to_vec()
            }
        };

        let keypair = Ed25519KeyPair::from_pkcs8(&pkcs8)
            .map_err(|_| invalid("the signing key file is corrupted"))?;
        let public_key = keypair.public_key().as_ref().to_vec();

        Ok(Self {
            keypair,
            public_key,
        })
    }

    pub fn public_key_hex(&self) -> String {
        hex_encode(&self.public_key)
    }

    pub fn public_key_base64(&self) -> String {
        base64url_encode(&self.public_key)
    }

    pub fn sign(&self, message: &[u8]) -> Vec<u8> {
        self.keypair.sign(message).as_ref().to_vec()
    }

    pub fn verify(&self, message: &[u8], signature: &[u8]) -> bool {
        UnparsedPublicKey::new(&ED25519, &self.public_key)
            .verify(message, signature)
            .is_ok()
    }
}

fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    fs::write(path, bytes)
}

fn invalid(message: &str) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, message.to_string())
}

pub fn random_bytes(len: usize) -> Vec<u8> {
    let mut buffer = vec![0u8; len];
    getrandom::fill(&mut buffer).expect("system randomness is unavailable");
    buffer
}
