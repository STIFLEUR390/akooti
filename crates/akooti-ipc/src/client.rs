//! Client used by `akooti mcp` and `akooti native-host`.
//!
//! The client holds no key and no vault handle: it forwards one request at a
//! time with the token it was configured with, and surfaces a typed error when
//! the daemon is gone. Each call opens its own connection, so a daemon restart
//! is transparent — there is no stale socket to invalidate.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::IpcError;
use crate::framing::{read_frame, write_frame};
use crate::protocol::{Handshake, PROTOCOL_VERSION, Request, Response, ResponseBody, VaultStatus};
use crate::socket::socket_path;

/// Connection and deadline settings.
#[derive(Debug, Clone)]
pub struct ClientConfig {
    /// Socket path.
    pub path: PathBuf,
    /// Connect + read + write deadline for one call.
    pub timeout: Duration,
}

impl ClientConfig {
    /// Configuration for the default socket.
    ///
    /// # Errors
    ///
    /// Returns [`IpcError::Unavailable`] when no socket path can be resolved.
    pub fn discover() -> Result<Self, IpcError> {
        let path = socket_path().map_err(|_| IpcError::Unavailable)?;
        Ok(Self {
            path,
            timeout: Duration::from_secs(10),
        })
    }

    /// Configuration for an explicit path, used by tests.
    #[must_use]
    pub fn at(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            timeout: Duration::from_secs(10),
        }
    }
}

/// One-request-at-a-time client.
#[derive(Debug, Clone)]
pub struct DaemonClient {
    config: ClientConfig,
}

impl DaemonClient {
    /// Builds a client for `config`.
    #[must_use]
    pub fn new(config: ClientConfig) -> Self {
        Self { config }
    }

    /// Socket the client talks to.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.config.path
    }

    /// Negotiates the protocol version with the daemon.
    ///
    /// # Errors
    ///
    /// Returns [`IpcError::VersionMismatch`] when the daemon speaks another
    /// version, and [`IpcError::Unavailable`] when it is not running.
    pub fn handshake(&self, client: &str) -> Result<Handshake, IpcError> {
        let body = self.call(&Request::Handshake {
            protocol_version: PROTOCOL_VERSION,
            client: client.to_owned(),
        })?;
        match body {
            ResponseBody::Handshake(handshake) => {
                if handshake.protocol_version != PROTOCOL_VERSION {
                    return Err(IpcError::VersionMismatch {
                        found: handshake.protocol_version,
                        expected: PROTOCOL_VERSION,
                    });
                }
                Ok(handshake)
            }
            ResponseBody::Error { code, message } => Err(IpcError::Remote { code, message }),
            _ => Err(IpcError::Protocol),
        }
    }

    /// Introspects the token without touching an item.
    ///
    /// # Errors
    ///
    /// Propagates transport and authentication errors.
    pub fn status(&self, token: &str) -> Result<VaultStatus, IpcError> {
        match self.call(&Request::VaultStatus {
            token: token.to_owned(),
        })? {
            ResponseBody::VaultStatus(status) => Ok(status),
            other => Err(unexpected(&other)),
        }
    }

    /// Sends one request and returns its body.
    ///
    /// # Errors
    ///
    /// Retries once after a connection loss (a daemon that just restarted), then
    /// surfaces [`IpcError::Unavailable`].
    pub fn call(&self, request: &Request) -> Result<ResponseBody, IpcError> {
        match self.exchange(request) {
            Ok(body) => Ok(body),
            Err(IpcError::Unavailable) => self.exchange(request),
            Err(other) => Err(other),
        }
    }

    fn exchange(&self, request: &Request) -> Result<ResponseBody, IpcError> {
        let payload = serde_json::to_vec(request).map_err(|_| IpcError::Protocol)?;
        let mut stream = connect(&self.config)?;
        stream
            .set_read_timeout(Some(self.config.timeout))
            .map_err(IpcError::Io)?;
        stream
            .set_write_timeout(Some(self.config.timeout))
            .map_err(IpcError::Io)?;
        write_frame(&mut stream, &payload)?;
        let frame = read_frame(&mut stream)?;
        let response: Response = serde_json::from_slice(&frame).map_err(|_| IpcError::Protocol)?;
        if response.protocol_version != PROTOCOL_VERSION {
            return Err(IpcError::VersionMismatch {
                found: response.protocol_version,
                expected: PROTOCOL_VERSION,
            });
        }
        match response.body {
            body @ ResponseBody::Error { .. } => match body {
                ResponseBody::Error { code, message } => Err(IpcError::Remote { code, message }),
                other => Ok(other),
            },
            body => Ok(body),
        }
    }
}

fn unexpected(body: &ResponseBody) -> IpcError {
    match body {
        ResponseBody::Error { code, message } => IpcError::Remote {
            code: code.clone(),
            message: message.clone(),
        },
        _ => IpcError::Protocol,
    }
}

#[cfg(unix)]
fn connect(config: &ClientConfig) -> Result<std::os::unix::net::UnixStream, IpcError> {
    let stream = std::os::unix::net::UnixStream::connect(&config.path).map_err(IpcError::from)?;
    stream
        .set_write_timeout(Some(config.timeout))
        .map_err(IpcError::Io)?;
    Ok(stream)
}

#[cfg(not(unix))]
fn connect(_config: &ClientConfig) -> Result<(), IpcError> {
    Err(IpcError::Unavailable)
}

/// Helper used by tests and by the daemon: writes one frame and reads one back.
///
/// # Errors
///
/// Propagates framing and IO failures.
pub fn serve_once<S: std::io::Read + Write>(
    stream: &mut S,
    handler: impl FnOnce(&[u8]) -> Vec<u8>,
) -> Result<(), IpcError> {
    let frame = read_frame(stream)?;
    let reply = handler(&frame);
    write_frame(stream, &reply)
}

#[cfg(all(test, unix))]
mod tests {
    use std::io::Write;
    use std::os::unix::net::UnixListener;
    use std::thread;

    use super::{ClientConfig, DaemonClient, serve_once};
    use crate::IpcError;
    use crate::protocol::{PROTOCOL_VERSION, Request, Response, ResponseBody, VaultStatus};

    fn fake_daemon(
        path: &std::path::Path,
        reply: Response,
        requests: usize,
    ) -> thread::JoinHandle<()> {
        let Ok(listener) = UnixListener::bind(path) else {
            return thread::spawn(|| ());
        };
        thread::spawn(move || {
            for _ in 0..requests {
                let Ok((mut stream, _)) = listener.accept() else {
                    return;
                };
                let _ = serve_once(&mut stream, |_| {
                    serde_json::to_vec(&reply).unwrap_or_default()
                });
            }
        })
    }

    #[test]
    fn a_call_round_trips_through_the_socket() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("daemon.sock");
        let reply = Response::ok(ResponseBody::VaultStatus(VaultStatus {
            token_name: "agent".to_owned(),
            scopes: vec!["read_metadata".to_owned()],
            folder_ids: vec!["folder".to_owned()],
            expires_at: None,
        }));
        let server = fake_daemon(&path, reply, 1);

        let client = DaemonClient::new(ClientConfig::at(&path));
        let status = client.status("ak_mcp_test").expect("status");
        assert_eq!(status.token_name, "agent");
        server.join().expect("join");
    }

    #[test]
    fn a_missing_daemon_is_reported_as_unavailable() {
        let temp = tempfile::tempdir().expect("tempdir");
        let client = DaemonClient::new(ClientConfig::at(temp.path().join("absent.sock")));
        assert!(matches!(
            client.call(&Request::VaultStatus {
                token: "t".to_owned()
            }),
            Err(IpcError::Unavailable)
        ));
    }

    #[test]
    fn remote_errors_keep_their_code() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("daemon.sock");
        let server = fake_daemon(&path, Response::error("forbidden", "access denied"), 1);
        let client = DaemonClient::new(ClientConfig::at(&path));
        match client.status("t") {
            Err(IpcError::Remote { code, message }) => {
                assert_eq!(code, "forbidden");
                assert_eq!(message, "access denied");
            }
            other => panic!("expected a remote error, got {other:?}"),
        }
        server.join().expect("join");
    }

    #[test]
    fn a_version_mismatch_is_rejected() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("daemon.sock");
        let reply = Response {
            protocol_version: PROTOCOL_VERSION + 1,
            body: ResponseBody::VaultStatus(VaultStatus {
                token_name: "agent".to_owned(),
                scopes: Vec::new(),
                folder_ids: Vec::new(),
                expires_at: None,
            }),
        };
        let server = fake_daemon(&path, reply, 1);
        let client = DaemonClient::new(ClientConfig::at(&path));
        assert!(matches!(
            client.status("t"),
            Err(IpcError::VersionMismatch { .. })
        ));
        server.join().expect("join");
    }

    #[test]
    fn the_client_retries_once_when_the_first_daemon_is_gone() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("daemon.sock");
        let reply = Response::ok(ResponseBody::VaultStatus(VaultStatus {
            token_name: "second".to_owned(),
            scopes: Vec::new(),
            folder_ids: Vec::new(),
            expires_at: None,
        }));
        // The single-shot fake daemon only answers the second attempt: the first
        // connection is refused, which the client must retry.
        let server = fake_daemon(&path, reply, 1);
        let client = DaemonClient::new(ClientConfig::at(&path));
        let status = client.status("t").expect("status");
        assert_eq!(status.token_name, "second");
        server.join().expect("join");
    }

    #[test]
    fn the_server_helper_reads_a_frame_and_writes_one_back() {
        let (mut client_side, server_side) = std::os::unix::net::UnixStream::pair().expect("pair");
        let handle = thread::spawn(move || {
            let mut server_side = server_side;
            serve_once(&mut server_side, |frame| {
                assert_eq!(frame, b"ping");
                b"pong".to_vec()
            })
            .expect("serve");
        });
        crate::framing::write_frame(&mut client_side, b"ping").expect("write");
        client_side.flush().expect("flush");
        let reply = crate::framing::read_frame(&mut client_side).expect("read");
        assert_eq!(reply, b"pong");
        handle.join().expect("join");
    }
}
