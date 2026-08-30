use serde::{Deserialize, Serialize};

pub const KEY_GROUPS: usize = 4;
pub const KEY_GROUP_LEN: usize = 5;
pub const ALPHABET: &[u8] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Activation {
    pub hwid: String,
    pub first_seen: u64,
    pub last_seen: u64,
    #[serde(default)]
    pub checks: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct License {
    #[serde(default)]
    pub key_hash: String,
    #[serde(default)]
    pub key_prefix: String,
    #[serde(default, rename = "key", skip_serializing)]
    pub legacy_key: Option<String>,
    pub product: String,
    pub plan: String,
    #[serde(default)]
    pub owner_id: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
    pub created_at: u64,
    #[serde(default)]
    pub duration_secs: Option<u64>,
    #[serde(default)]
    pub expires_at: Option<u64>,
    #[serde(default = "one")]
    pub max_activations: u32,
    #[serde(default)]
    pub activations: Vec<Activation>,
    #[serde(default)]
    pub revoked: bool,
    #[serde(default)]
    pub revoked_reason: Option<String>,
    #[serde(default)]
    pub issued_by: Option<String>,
    #[serde(default)]
    pub last_check: Option<u64>,
}

fn one() -> u32 {
    1
}

impl License {
    pub fn is_lifetime(&self) -> bool {
        self.expires_at.is_none()
    }

    pub fn is_expired(&self, now: u64) -> bool {
        matches!(self.expires_at, Some(deadline) if deadline <= now)
    }

    pub fn status(&self, now: u64) -> LicenseStatus {
        if self.revoked {
            LicenseStatus::Revoked
        } else if self.is_expired(now) {
            LicenseStatus::Expired
        } else if self.activations.is_empty() {
            LicenseStatus::Unused
        } else {
            LicenseStatus::Active
        }
    }

    pub fn activation(&self, hwid: &str) -> Option<&Activation> {
        self.activations.iter().find(|a| a.hwid == hwid)
    }

    pub fn remaining(&self, now: u64) -> Option<u64> {
        self.expires_at.map(|deadline| deadline.saturating_sub(now))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LicenseStatus {
    Unused,
    Active,
    Expired,
    Revoked,
}

impl LicenseStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            LicenseStatus::Unused => "unused",
            LicenseStatus::Active => "active",
            LicenseStatus::Expired => "expired",
            LicenseStatus::Revoked => "revoked",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LicenseError {
    UnknownKey,
    AmbiguousKey,
    Revoked,
    Expired,
    HardwareLimit,
    InvalidHardware,
    InvalidToken,
    InvalidRequest,
}

impl LicenseError {
    pub fn code(self) -> &'static str {
        match self {
            LicenseError::UnknownKey => "unknown_key",
            LicenseError::AmbiguousKey => "ambiguous_key",
            LicenseError::Revoked => "revoked",
            LicenseError::Expired => "expired",
            LicenseError::HardwareLimit => "hardware_limit",
            LicenseError::InvalidHardware => "invalid_hardware",
            LicenseError::InvalidToken => "invalid_token",
            LicenseError::InvalidRequest => "invalid_request",
        }
    }

    pub fn message(self) -> &'static str {
        match self {
            LicenseError::UnknownKey => "This licence key does not exist",
            LicenseError::AmbiguousKey => "Several licences share this prefix, use the full key",
            LicenseError::Revoked => "This licence has been revoked",
            LicenseError::Expired => "This licence has expired",
            LicenseError::HardwareLimit => "This licence is already bound to another machine",
            LicenseError::InvalidHardware => "The hardware identifier does not match",
            LicenseError::InvalidToken => "The licence token is invalid or corrupted",
            LicenseError::InvalidRequest => "The request payload is invalid",
        }
    }

    pub fn http_status(self) -> u16 {
        match self {
            LicenseError::UnknownKey => 404,
            LicenseError::AmbiguousKey => 409,
            LicenseError::InvalidRequest => 400,
            _ => 403,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenPayload {
    pub v: u8,
    pub key: String,
    pub key_prefix: String,
    pub product: String,
    pub plan: String,
    pub hwid: String,
    pub owner: Option<String>,
    pub issued_at: u64,
    pub expires_at: u64,
    pub offline_until: u64,
    pub nonce: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct GrantedToken {
    pub token: String,
    pub expires_at: u64,
    pub offline_until: u64,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct Stats {
    pub total: usize,
    pub active: usize,
    pub unused: usize,
    pub expired: usize,
    pub revoked: usize,
    pub machines: usize,
}
