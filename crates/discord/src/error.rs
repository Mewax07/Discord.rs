#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    Tls(rustls::Error),
    InvalidDnsName,
    Http(String),
    Json(serde_json::Error),
    WebSocket(String),
    Protocol(&'static str),
}

pub type Result<T> = std::result::Result<T, Error>;

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Io(e) => write!(f, "io error: {e}"),
            Error::Tls(e) => write!(f, "tls error: {e}"),
            Error::InvalidDnsName => write!(f, "invalid dns name"),
            Error::Http(s) => write!(f, "http error: {s}"),
            Error::Json(e) => write!(f, "json error: {e}"),
            Error::WebSocket(s) => write!(f, "websocket error: {s}"),
            Error::Protocol(s) => write!(f, "protocol error: {s}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

impl From<rustls::Error> for Error {
    fn from(e: rustls::Error) -> Self {
        Error::Tls(e)
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error::Json(e)
    }
}
