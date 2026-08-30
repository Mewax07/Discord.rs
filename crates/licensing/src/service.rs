use std::collections::HashSet;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::crypto::{
    base64url_decode, base64url_encode, hex_encode, random_bytes, sha256_hex, Signer,
};
use crate::model::{
    Activation, GrantedToken, License, LicenseError, LicenseStatus, Stats, TokenPayload, ALPHABET,
    KEY_GROUPS, KEY_GROUP_LEN,
};
use crate::store::LicenseStore;

pub const DEFAULT_OFFLINE_GRACE: u64 = 14 * 86_400;
const KEY_PREFIX: &str = "BDM";
const PREFIX_LEN: usize = 9;
const MAX_HWID_LEN: usize = 128;

pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[derive(Debug, Clone)]
pub struct Issued {
    pub license: License,
    pub key: String,
}

#[derive(Debug, Clone)]
pub struct IssueRequest {
    pub product: String,
    pub plan: String,
    pub duration_secs: Option<u64>,
    pub max_activations: u32,
    pub owner_id: Option<String>,
    pub note: Option<String>,
    pub issued_by: Option<String>,
}

impl IssueRequest {
    pub fn new(product: impl Into<String>, plan: impl Into<String>) -> Self {
        Self {
            product: product.into(),
            plan: plan.into(),
            duration_secs: None,
            max_activations: 1,
            owner_id: None,
            note: None,
            issued_by: None,
        }
    }

    pub fn duration(mut self, seconds: Option<u64>) -> Self {
        self.duration_secs = seconds;
        self
    }

    pub fn machines(mut self, count: u32) -> Self {
        self.max_activations = count.max(1);
        self
    }

    pub fn owner(mut self, owner_id: Option<String>) -> Self {
        self.owner_id = owner_id;
        self
    }

    pub fn note(mut self, note: Option<String>) -> Self {
        self.note = note;
        self
    }

    pub fn issued_by(mut self, actor: Option<String>) -> Self {
        self.issued_by = actor;
        self
    }
}

pub struct LicenseService {
    store: LicenseStore,
    signer: Signer,
    offline_grace: u64,
}

impl LicenseService {
    pub fn open(
        store_path: impl Into<PathBuf>,
        key_path: impl Into<PathBuf>,
        offline_grace: u64,
    ) -> std::io::Result<Self> {
        let service = Self {
            store: LicenseStore::open(store_path),
            signer: Signer::open(key_path)?,
            offline_grace: offline_grace.max(3_600),
        };
        service.migrate();
        Ok(service)
    }

    fn migrate(&self) {
        self.store.write(|data| {
            let stale: Vec<(String, License)> = data
                .licenses
                .iter()
                .filter(|(_, license)| license.key_hash.is_empty())
                .map(|(id, license)| (id.clone(), license.clone()))
                .collect();

            for (id, mut license) in stale {
                let plaintext = license.legacy_key.clone().unwrap_or_else(|| id.clone());
                license.key_hash = hash_key(&plaintext);
                license.key_prefix = key_prefix(&plaintext);
                license.legacy_key = None;
                data.licenses.remove(&id);
                data.licenses.insert(license.key_hash.clone(), license);
            }
        });
    }

    pub fn public_key_hex(&self) -> String {
        self.signer.public_key_hex()
    }

    pub fn public_key_base64(&self) -> String {
        self.signer.public_key_base64()
    }

    pub fn offline_grace(&self) -> u64 {
        self.offline_grace
    }

    pub fn issue(&self, request: IssueRequest) -> Issued {
        let now = now_secs();
        let key = self.unique_key();

        let license = License {
            key_hash: hash_key(&key),
            key_prefix: key_prefix(&key),
            legacy_key: None,
            product: request.product,
            plan: request.plan,
            owner_id: request.owner_id,
            note: request.note,
            created_at: now,
            duration_secs: request.duration_secs,
            expires_at: None,
            max_activations: request.max_activations.max(1),
            activations: Vec::new(),
            revoked: false,
            revoked_reason: None,
            issued_by: request.issued_by,
            last_check: None,
        };

        self.store.write(|data| {
            data.licenses
                .insert(license.key_hash.clone(), license.clone());
        });

        Issued { license, key }
    }

    pub fn get(&self, key: &str) -> Option<License> {
        let hash = hash_key(key);
        self.store.read(|data| data.licenses.get(&hash).cloned())
    }

    pub fn resolve(&self, reference: &str) -> Result<License, LicenseError> {
        if let Some(license) = self.get(reference) {
            return Ok(license);
        }

        let prefix = normalize_key(reference);
        if prefix.len() < 5 {
            return Err(LicenseError::UnknownKey);
        }

        let matches: Vec<License> = self.store.read(|data| {
            data.licenses
                .values()
                .filter(|license| license.key_prefix == prefix)
                .cloned()
                .collect()
        });

        match matches.len() {
            0 => Err(LicenseError::UnknownKey),
            1 => Ok(matches.into_iter().next().unwrap()),
            _ => Err(LicenseError::AmbiguousKey),
        }
    }

    pub fn for_owner(&self, owner_id: &str) -> Vec<License> {
        self.store.read(|data| {
            let mut found: Vec<License> = data
                .licenses
                .values()
                .filter(|license| license.owner_id.as_deref() == Some(owner_id))
                .cloned()
                .collect();
            found.sort_by(|a, b| b.created_at.cmp(&a.created_at));
            found
        })
    }

    pub fn activate(&self, key: &str, hwid: &str) -> Result<(License, GrantedToken), LicenseError> {
        let hash = hash_key(key);
        let hwid = normalize_hwid(hwid).ok_or(LicenseError::InvalidRequest)?;
        let now = now_secs();

        let license = self.store.write(|data| {
            let Some(license) = data.licenses.get_mut(&hash) else {
                return Err(LicenseError::UnknownKey);
            };
            if license.revoked {
                return Err(LicenseError::Revoked);
            }
            if license.is_expired(now) {
                return Err(LicenseError::Expired);
            }

            match license.activations.iter_mut().find(|a| a.hwid == hwid) {
                Some(activation) => {
                    activation.last_seen = now;
                    activation.checks += 1;
                }
                None => {
                    if license.activations.len() as u32 >= license.max_activations {
                        return Err(LicenseError::HardwareLimit);
                    }
                    license.activations.push(Activation {
                        hwid: hwid.clone(),
                        first_seen: now,
                        last_seen: now,
                        checks: 1,
                    });
                }
            }

            if license.expires_at.is_none() {
                if let Some(duration) = license.duration_secs {
                    license.expires_at = Some(now + duration);
                }
            }
            license.last_check = Some(now);

            Ok(license.clone())
        })?;

        let token = self.grant(&license, key, &hwid, now);
        Ok((license, token))
    }

    pub fn validate(&self, key: &str, hwid: &str) -> Result<License, LicenseError> {
        let hash = hash_key(key);
        let hwid = normalize_hwid(hwid).ok_or(LicenseError::InvalidRequest)?;
        let now = now_secs();

        self.store.write(|data| {
            let Some(license) = data.licenses.get_mut(&hash) else {
                return Err(LicenseError::UnknownKey);
            };
            if license.revoked {
                return Err(LicenseError::Revoked);
            }
            if license.is_expired(now) {
                return Err(LicenseError::Expired);
            }
            let Some(activation) = license.activations.iter_mut().find(|a| a.hwid == hwid) else {
                return Err(LicenseError::InvalidHardware);
            };

            activation.last_seen = now;
            activation.checks += 1;
            license.last_check = Some(now);

            Ok(license.clone())
        })
    }

    pub fn refresh(&self, key: &str, hwid: &str) -> Result<(License, GrantedToken), LicenseError> {
        let license = self.validate(key, hwid)?;
        let hwid = normalize_hwid(hwid).ok_or(LicenseError::InvalidRequest)?;
        let token = self.grant(&license, key, &hwid, now_secs());
        Ok((license, token))
    }

    pub fn verify_token(&self, token: &str) -> Result<TokenPayload, LicenseError> {
        let (body, signature) = token.split_once('.').ok_or(LicenseError::InvalidToken)?;
        let payload_bytes = base64url_decode(body).ok_or(LicenseError::InvalidToken)?;
        let signature_bytes = base64url_decode(signature).ok_or(LicenseError::InvalidToken)?;

        if !self.signer.verify(&payload_bytes, &signature_bytes) {
            return Err(LicenseError::InvalidToken);
        }

        let payload: TokenPayload =
            serde_json::from_slice(&payload_bytes).map_err(|_| LicenseError::InvalidToken)?;

        let now = now_secs();
        if payload.expires_at != 0 && payload.expires_at <= now {
            return Err(LicenseError::Expired);
        }

        match self.get(&payload.key) {
            Some(license) if license.revoked => Err(LicenseError::Revoked),
            Some(_) | None => Ok(payload),
        }
    }

    pub fn revoke(&self, reference: &str, reason: Option<String>) -> Result<License, LicenseError> {
        self.mutate(reference, |license| {
            license.revoked = true;
            license.revoked_reason = reason.clone();
        })
    }

    pub fn restore(&self, reference: &str) -> Result<License, LicenseError> {
        self.mutate(reference, |license| {
            license.revoked = false;
            license.revoked_reason = None;
        })
    }

    pub fn reset_hardware(&self, reference: &str) -> Result<License, LicenseError> {
        self.mutate(reference, |license| license.activations.clear())
    }

    pub fn assign(
        &self,
        reference: &str,
        owner_id: Option<String>,
    ) -> Result<License, LicenseError> {
        self.mutate(reference, |license| license.owner_id = owner_id.clone())
    }

    fn mutate(&self, reference: &str, f: impl Fn(&mut License)) -> Result<License, LicenseError> {
        let hash = self.resolve(reference)?.key_hash;
        self.store.write(|data| {
            let Some(license) = data.licenses.get_mut(&hash) else {
                return Err(LicenseError::UnknownKey);
            };
            f(license);
            Ok(license.clone())
        })
    }

    pub fn stats(&self) -> Stats {
        let now = now_secs();
        self.store.read(|data| {
            let mut stats = Stats {
                total: data.licenses.len(),
                ..Stats::default()
            };
            let mut machines = HashSet::new();

            for license in data.licenses.values() {
                match license.status(now) {
                    LicenseStatus::Active => stats.active += 1,
                    LicenseStatus::Unused => stats.unused += 1,
                    LicenseStatus::Expired => stats.expired += 1,
                    LicenseStatus::Revoked => stats.revoked += 1,
                }
                for activation in &license.activations {
                    machines.insert(activation.hwid.clone());
                }
            }

            stats.machines = machines.len();
            stats
        })
    }

    fn grant(&self, license: &License, key: &str, hwid: &str, now: u64) -> GrantedToken {
        let expires_at = license.expires_at.unwrap_or(0);
        let offline_until = match expires_at {
            0 => now + self.offline_grace,
            deadline => (now + self.offline_grace).min(deadline),
        };

        let payload = TokenPayload {
            v: 1,
            key: normalize_key(key),
            key_prefix: license.key_prefix.clone(),
            product: license.product.clone(),
            plan: license.plan.clone(),
            hwid: hwid.to_string(),
            owner: license.owner_id.clone(),
            issued_at: now,
            expires_at,
            offline_until,
            nonce: hex_encode(&random_bytes(12)),
        };

        let body = serde_json::to_vec(&payload).expect("token serialization");
        let signature = self.signer.sign(&body);

        GrantedToken {
            token: format!(
                "{}.{}",
                base64url_encode(&body),
                base64url_encode(&signature)
            ),
            expires_at,
            offline_until,
        }
    }

    fn unique_key(&self) -> String {
        loop {
            let candidate = generate_key();
            let hash = hash_key(&candidate);
            let prefix = key_prefix(&candidate);

            let taken = self.store.read(|data| {
                data.licenses.contains_key(&hash)
                    || data
                        .licenses
                        .values()
                        .any(|license| license.key_prefix == prefix)
            });

            if !taken {
                return candidate;
            }
        }
    }
}

pub fn generate_key() -> String {
    let bytes = random_bytes(KEY_GROUPS * KEY_GROUP_LEN);
    let mut key = String::from(KEY_PREFIX);

    for (index, byte) in bytes.iter().enumerate() {
        if index % KEY_GROUP_LEN == 0 {
            key.push('-');
        }
        key.push(ALPHABET[(*byte as usize) % ALPHABET.len()] as char);
    }

    key
}

pub fn hash_key(key: &str) -> String {
    sha256_hex(&normalize_key(key))
}

pub fn key_prefix(key: &str) -> String {
    normalize_key(key).chars().take(PREFIX_LEN).collect()
}

pub fn normalize_key(key: &str) -> String {
    key.trim()
        .to_ascii_uppercase()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .collect()
}

pub fn normalize_hwid(hwid: &str) -> Option<String> {
    let cleaned: String = hwid
        .trim()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_' || *c == ':')
        .take(MAX_HWID_LEN)
        .collect();

    (cleaned.len() >= 8).then_some(cleaned)
}
