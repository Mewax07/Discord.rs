pub mod crypto;
pub mod http;
pub mod model;
pub mod offline;
pub mod service;
pub mod store;

pub use http::{spawn as spawn_api, ApiConfig};
pub use model::{
    Activation, GrantedToken, License, LicenseError, LicenseStatus, Stats, TokenPayload,
};
pub use offline::{verify as verify_offline, verify_with_hex_key as verify_offline_hex};
pub use service::{
    generate_key, hash_key, key_prefix, normalize_hwid, normalize_key, now_secs, IssueRequest,
    Issued, LicenseService, DEFAULT_OFFLINE_GRACE,
};
pub use store::LicenseStore;
