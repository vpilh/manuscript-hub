//! TCP plus TLS. Certificates are checked against the operating system's trust
//! store; there is no option to skip verification and no plaintext fallback.

use std::io::{self, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use rustls::pki_types::ServerName;
use rustls::{ClientConfig, ClientConnection, StreamOwned};
use rustls_platform_verifier::ConfigVerifierExt;

use super::session::Session;
use super::{ImapError, ImapSettings, Security};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const IO_TIMEOUT: Duration = Duration::from_secs(60);

pub(crate) trait Transport: Read + Write + Send {}
impl<T: Read + Write + Send> Transport for T {}

pub(crate) type Stream = Box<dyn Transport>;

/// Opens an authenticated-ready session: connected, encrypted, greeted.
pub(crate) fn connect(settings: &ImapSettings) -> Result<Session<Stream>, ImapError> {
    let tcp = tcp(&settings.host, settings.port)?;
    match settings.security {
        Security::Tls => {
            let tls = tls(&settings.host, tcp)?;
            Session::greet(Box::new(tls) as Stream)
        }
        Security::StartTls => {
            let mut plain = Session::greet(tcp)?;
            plain.starttls()?;
            let tls = tls(&settings.host, plain.into_stream()?)?;
            let mut session = Session::resume(Box::new(tls) as Stream);
            // Capabilities seen before TLS can't be trusted; ask again.
            session.refresh_capabilities()?;
            Ok(session)
        }
    }
}

fn tcp(host: &str, port: u16) -> Result<TcpStream, ImapError> {
    let addresses = (host, port)
        .to_socket_addrs()
        .map_err(|_| ImapError::HostNotFound(host.to_string()))?;
    let mut last_error = None;
    for address in addresses {
        match TcpStream::connect_timeout(&address, CONNECT_TIMEOUT) {
            Ok(stream) => {
                stream.set_read_timeout(Some(IO_TIMEOUT))?;
                stream.set_write_timeout(Some(IO_TIMEOUT))?;
                return Ok(stream);
            }
            Err(e) => last_error = Some(e),
        }
    }
    Err(ImapError::Unreachable {
        host: host.to_string(),
        port,
        reason: last_error.map_or_else(|| "no address".into(), |e| e.to_string()),
    })
}

fn tls(
    host: &str,
    mut tcp: TcpStream,
) -> Result<StreamOwned<ClientConnection, TcpStream>, ImapError> {
    let name = ServerName::try_from(host.to_string())
        .map_err(|_| ImapError::HostNotFound(host.to_string()))?;
    let mut connection =
        ClientConnection::new(client_config()?, name).map_err(|e| ImapError::Tls(e.to_string()))?;
    // Finish the handshake now, so certificate problems surface as such
    // rather than as a failed first read.
    while connection.is_handshaking() {
        connection.complete_io(&mut tcp).map_err(tls_error)?;
    }
    Ok(StreamOwned::new(connection, tcp))
}

fn client_config() -> Result<Arc<ClientConfig>, ImapError> {
    static CONFIG: OnceLock<Result<Arc<ClientConfig>, String>> = OnceLock::new();
    CONFIG
        .get_or_init(|| {
            ClientConfig::with_platform_verifier()
                .map(Arc::new)
                .map_err(|e| e.to_string())
        })
        .clone()
        .map_err(ImapError::Tls)
}

fn tls_error(e: io::Error) -> ImapError {
    match e
        .get_ref()
        .and_then(|inner| inner.downcast_ref::<rustls::Error>())
    {
        Some(rustls::Error::InvalidCertificate(reason)) => {
            ImapError::Certificate(format!("{reason:?}"))
        }
        Some(other) => ImapError::Tls(other.to_string()),
        None => ImapError::Io(e),
    }
}
