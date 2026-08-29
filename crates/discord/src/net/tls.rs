use std::{
    io::{self, Read, Write},
    net::TcpStream,
    sync::Arc,
    time::Duration,
};

use rustls::{pki_types::ServerName, ClientConfig, ClientConnection, RootCertStore, StreamOwned};

use crate::{Error, Result};

pub struct TlsStream {
    inner: StreamOwned<ClientConnection, TcpStream>,
}

impl TlsStream {
    pub fn connect(host: &str, port: u16) -> Result<Self> {
        let config = client_config();
        let server_name =
            ServerName::try_from(host.to_string()).map_err(|_| Error::InvalidDnsName)?;
        let conn = ClientConnection::new(config, server_name)?;
        let tcp = TcpStream::connect((host, port))?;
        tcp.set_nodelay(true).ok();

        Ok(Self {
            inner: StreamOwned::new(conn, tcp),
        })
    }

    pub fn set_read_timeout(&self, dur: Option<Duration>) -> io::Result<()> {
        self.inner.sock.set_read_timeout(dur)
    }
}

impl Read for TlsStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.inner.read(buf)
    }
}

impl Write for TlsStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.inner.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

fn root_store() -> RootCertStore {
    let mut store = RootCertStore::empty();
    store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    store
}

fn client_config() -> Arc<ClientConfig> {
    Arc::new(
        ClientConfig::builder()
            .with_root_certificates(root_store())
            .with_no_client_auth(),
    )
}
