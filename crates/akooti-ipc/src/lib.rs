//! Local IPC between the GUI daemon and the headless entrypoints.
//!
//! Two rules shape this crate (`docs/architecture.md` §1.2):
//!
//! 1. **No secret crosses this boundary towards an agent.** The daemon answers
//!    `vault_search` with metadata and `*_autofill` with an outcome; values only
//!    travel into the browser or a child process.
//! 2. **No TCP.** The transport is a Unix socket owned by the user (or a named
//!    pipe on Windows), created with restrictive permissions before the first
//!    connection.
//!
//! The protocol is length-prefixed JSON: 4-byte little-endian length, then one
//! UTF-8 JSON document, capped at [`MAX_FRAME_BYTES`].

#![forbid(unsafe_code)]

pub mod client;
pub mod framing;
pub mod protocol;
pub mod socket;

pub use client::{ClientConfig, DaemonClient};
pub use framing::{MAX_FRAME_BYTES, read_frame, write_frame};
pub use protocol::{
    ExecOutcome, FillOutcome, FillPayload, FillStatus, Handshake, ItemSummary, PROTOCOL_VERSION,
    PasskeyCredential, PasskeySignature, Request, Response, ResponseBody, SelectorOverride,
    VaultStatus,
};
pub use socket::{SocketError, ensure_socket_dir, socket_path, socket_path_in};

/// Errors surfaced by the IPC layer.
#[derive(Debug, thiserror::Error)]
pub enum IpcError {
    /// The daemon is not running, or the socket disappeared.
    #[error("the Akooti daemon is not running")]
    Unavailable,
    /// The daemon answered with an error envelope.
    #[error("{message}")]
    Remote {
        /// Stable machine-readable code.
        code: String,
        /// Human-readable message, safe to display.
        message: String,
    },
    /// The peer spoke a different protocol version.
    #[error("the Akooti daemon speaks protocol {found}, this client needs {expected}")]
    VersionMismatch {
        /// Version announced by the daemon.
        found: u32,
        /// Version requested by the client.
        expected: u32,
    },
    /// The frame was malformed or too large.
    #[error("the IPC frame is invalid")]
    Protocol,
    /// The transport failed.
    #[error("local IPC failed")]
    Io(#[source] std::io::Error),
    /// The daemon did not answer within the deadline.
    #[error("the Akooti daemon did not answer in time")]
    Timeout,
}

impl From<std::io::Error> for IpcError {
    fn from(error: std::io::Error) -> Self {
        match error.kind() {
            std::io::ErrorKind::NotFound
            | std::io::ErrorKind::ConnectionRefused
            | std::io::ErrorKind::ConnectionReset
            | std::io::ErrorKind::BrokenPipe => Self::Unavailable,
            std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock => Self::Timeout,
            _ => Self::Io(error),
        }
    }
}
