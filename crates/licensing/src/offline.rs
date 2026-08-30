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
