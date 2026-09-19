use ring::signature::{UnparsedPublicKey, ED25519};

use crate::crypto::{base64url_decode, hex_decode};
use crate::model::{LicenseError, TokenPayload};

pub fn verify(
    public_key: &[u8],
    token: &str,
    hwid: &str,
    now: u64,
) -> Result<TokenPayload, LicenseError> {
    let (body, signature) = token.split_once('.').ok_or(LicenseError::InvalidToken)?;
    let payload_bytes = base64url_decode(body).ok_or(LicenseError::InvalidToken)?;
    let signature_bytes = base64url_decode(signature).ok_or(LicenseError::InvalidToken)?;

    UnparsedPublicKey::new(&ED25519, public_key)
        .verify(&payload_bytes, &signature_bytes)
        .map_err(|_| LicenseError::InvalidToken)?;

    let payload: TokenPayload =
        serde_json::from_slice(&payload_bytes).map_err(|_| LicenseError::InvalidToken)?;

    if payload.hwid != hwid {
        return Err(LicenseError::InvalidHardware);
    }
    if payload.expires_at != 0 && payload.expires_at <= now {
        return Err(LicenseError::Expired);
    }
    if payload.offline_until <= now {
        return Err(LicenseError::InvalidToken);
    }

    Ok(payload)
}

pub fn verify_with_hex_key(
    public_key_hex: &str,
    token: &str,
    hwid: &str,
    now: u64,
) -> Result<TokenPayload, LicenseError> {
    let key = hex_decode(public_key_hex).ok_or(LicenseError::InvalidToken)?;
    verify(&key, token, hwid, now)
}

#[derive(Debug, Clone)]
pub enum Verdict {
    Valid(TokenPayload),
    Expired(TokenPayload),
    GraceClosed(TokenPayload),
    WrongHardware,
    Forged,
}

pub fn inspect(public_key: &[u8], token: &str, hwid: &str, now: u64) -> Verdict {
    let Some((body, signature)) = token.split_once('.') else {
        return Verdict::Forged;
    };
    let (Some(payload_bytes), Some(signature_bytes)) =
        (base64url_decode(body), base64url_decode(signature))
    else {
        return Verdict::Forged;
    };

    if UnparsedPublicKey::new(&ED25519, public_key)
        .verify(&payload_bytes, &signature_bytes)
        .is_err()
    {
        return Verdict::Forged;
    }

    let Ok(payload) = serde_json::from_slice::<TokenPayload>(&payload_bytes) else {
        return Verdict::Forged;
    };

    if payload.hwid != hwid {
        return Verdict::WrongHardware;
    }
    if payload.expires_at != 0 && payload.expires_at <= now {
        return Verdict::Expired(payload);
    }
    if payload.offline_until <= now {
        return Verdict::GraceClosed(payload);
    }
    Verdict::Valid(payload)
}

pub fn inspect_with_hex_key(public_key_hex: &str, token: &str, hwid: &str, now: u64) -> Verdict {
    match hex_decode(public_key_hex) {
        Some(key) => inspect(&key, token, hwid, now),
        None => Verdict::Forged,
    }
}
