//! `akooti native-host`: the Native Messaging bridge of the browser extension.
//!
//! The host is a **frame pump**, not a vault client: it copies length-prefixed
//! JSON frames between the browser (stdin/stdout, exactly the Native Messaging
//! framing the browser already applies) and the local daemon socket, in both
//! directions. It never decodes a frame, never holds the key, never logs and
//! never buffers a value in a string, so the contract of
//! [`docs/architecture.md`](../../docs/architecture.md) §2 holds: this process
//! cannot read a secret, it can only relay bytes between two peers it does not
//! understand.
//!
//! One connection serves one browser session. The extension reconnects (see
//! `apps/extension/lib/native/bridge.ts`) when the daemon restarts, and the pump
//! exits as soon as either side closes, which is what the browser expects from
//! a Native Messaging host.

use std::io::{Read, Write};

use akooti_ipc::{ClientConfig, Response, read_frame, write_frame};

/// The pump ended because the browser closed the port or the daemon stopped.
pub const EXIT_OK: u8 = 0;
/// The pump could not start or the sockets failed.
pub const EXIT_FAILURE: u8 = 1;

/// Why a pump session ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PumpEnd {
    /// The browser closed stdin: the port is gone, stop quietly.
    BrowserClosed,
    /// The daemon closed the socket: the app quit or restarted.
    DaemonClosed,
    /// A frame could not be read or written.
    Failed,
}

impl PumpEnd {
    /// Exit code matching the end of the session.
    #[must_use]
    pub const fn exit_code(self) -> u8 {
        match self {
            // A closed port is a normal end of life for a Native Messaging
            // host: browsers stop the process whenever the extension context
            // goes away.
            Self::BrowserClosed | Self::DaemonClosed => EXIT_OK,
            Self::Failed => EXIT_FAILURE,
        }
    }
}

/// Runs the bridge on stdin/stdout and returns the process exit code.
pub fn run() -> u8 {
    #[cfg(unix)]
    {
        let config = match ClientConfig::discover() {
            Ok(config) => config,
            Err(_) => {
                emit_unavailable(&mut std::io::stdout());
                return EXIT_FAILURE;
            }
        };
        match connect(&config.path) {
            Ok(stream) => pump(std::io::stdin(), std::io::stdout(), stream).exit_code(),
            Err(_) => {
                emit_unavailable(&mut std::io::stdout());
                EXIT_FAILURE
            }
        }
    }
    #[cfg(not(unix))]
    {
        emit_unavailable(&mut std::io::stdout());
        EXIT_FAILURE
    }
}

/// Opens the daemon socket.
///
/// There is no read timeout: an idle bridge legitimately waits for the next
/// fill request. Only the write side must not block forever.
#[cfg(unix)]
fn connect(path: &std::path::Path) -> std::io::Result<std::os::unix::net::UnixStream> {
    let stream = std::os::unix::net::UnixStream::connect(path)?;
    stream.set_write_timeout(Some(std::time::Duration::from_secs(10)))?;
    Ok(stream)
}

/// Tells the extension why no bridge exists, using the daemon's error shape so
/// the extension shows one message instead of a dead port.
fn emit_unavailable(out: &mut impl Write) {
    let response = Response::error(
        "daemon_unavailable",
        "the Akooti desktop app is not running",
    );
    if let Ok(encoded) = serde_json::to_vec(&response) {
        let _ = write_frame(out, &encoded);
        let _ = out.flush();
    }
}

/// Copies frames in both directions until one side closes.
///
/// The input→socket half runs in its own thread so that a browser message and a
/// daemon message can cross at the same time. When the browser closes the port,
/// the write half is shut down: the daemon ends the session and closes the
/// socket, which releases the read half of this pump.
#[cfg(unix)]
pub fn pump<R, W>(input: R, mut output: W, stream: std::os::unix::net::UnixStream) -> PumpEnd
where
    R: Read + Send + 'static,
    W: Write,
{
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc::channel;

    let Ok(mut writer) = stream.try_clone() else {
        return PumpEnd::Failed;
    };
    let browser_closed = Arc::new(AtomicBool::new(false));
    let (ready_tx, ready_rx) = channel::<()>();
    let forwarded_flag = Arc::clone(&browser_closed);
    let forwarder = std::thread::Builder::new()
        .name("akooti-native-host-in".to_owned())
        .spawn(move || {
            let _ = ready_tx.send(());
            let mut input = input;
            while let Ok(frame) = read_frame(&mut input) {
                if write_frame(&mut writer, &frame).is_err() {
                    break;
                }
            }
            // The port is gone: tell the daemon, then let the read half close.
            forwarded_flag.store(true, Ordering::SeqCst);
            let _ = writer.shutdown(std::net::Shutdown::Write);
        });
    let Ok(forwarder) = forwarder else {
        return PumpEnd::Failed;
    };
    let _ = ready_rx.recv();

    let mut reader = stream;
    let end = loop {
        let frame = match read_frame(&mut reader) {
            Ok(frame) => frame,
            Err(error) => {
                break if browser_closed.load(Ordering::SeqCst) {
                    PumpEnd::BrowserClosed
                } else if matches!(
                    error,
                    akooti_ipc::IpcError::Io(_) | akooti_ipc::IpcError::Unavailable
                ) {
                    PumpEnd::DaemonClosed
                } else {
                    PumpEnd::Failed
                };
            }
        };
        if write_frame(&mut output, &frame).is_err() || output.flush().is_err() {
            break PumpEnd::BrowserClosed;
        }
    };
    let _ = reader.shutdown(std::net::Shutdown::Read);
    // The input half is not joined: a blocking read on stdin cannot be
    // interrupted, so when the daemon goes away first, joining would keep the
    // bridge alive forever instead of telling the browser the port is gone.
    // `run` returns right after this and the process exit takes the thread.
    drop(forwarder);
    end
}

#[cfg(test)]
mod tests {
    use super::{PumpEnd, emit_unavailable, pump};
    use akooti_ipc::{MAX_FRAME_BYTES, read_frame, write_frame};
    use std::io::Cursor;
    use std::os::unix::net::UnixStream;
    use std::sync::{Arc, Mutex};

    /// Collects what the pump writes towards the browser.
    #[derive(Clone, Default)]
    struct Sink(Arc<Mutex<Vec<u8>>>);

    impl std::io::Write for Sink {
        fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
            self.0
                .lock()
                .map_err(|_| std::io::Error::other("poisoned"))?
                .extend_from_slice(buffer);
            Ok(buffer.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl Sink {
        fn frames(&self) -> Vec<Vec<u8>> {
            let mut reader =
                Cursor::new(self.0.lock().map(|guard| guard.clone()).unwrap_or_default());
            let mut frames = Vec::new();
            while let Ok(frame) = read_frame(&mut reader) {
                frames.push(frame);
            }
            frames
        }
    }

    #[test]
    fn frames_cross_the_bridge_untouched_in_both_directions() {
        let (daemon_side, bridge_side) = UnixStream::pair().expect("pair");
        // The browser port stays open while the daemon answers, like a real
        // extension whose worker keeps its stdin alive.
        let (mut browser_port, browser_input) = UnixStream::pair().expect("pair");
        let sink = Sink::default();
        // A request from the extension, with a value that must not be altered.
        write_frame(
            &mut browser_port,
            br#"{"method":"vaultStatus","token":"ak_mcp_probe"}"#,
        )
        .expect("frame");

        let daemon = std::thread::spawn(move || {
            let mut daemon_side = daemon_side;
            let received = read_frame(&mut daemon_side).expect("daemon read");
            assert_eq!(
                received,
                br#"{"method":"vaultStatus","token":"ak_mcp_probe"}"#
            );
            write_frame(
                &mut daemon_side,
                br#"{"kind":"vaultStatus","tokenName":"probe"}"#,
            )
            .expect("daemon write");
        });

        let end = pump(browser_input, sink.clone(), bridge_side);
        daemon.join().expect("daemon thread");
        assert_eq!(end, PumpEnd::DaemonClosed);
        assert_eq!(
            sink.frames(),
            vec![br#"{"kind":"vaultStatus","tokenName":"probe"}"#.to_vec()]
        );
        // Releasing the port is what stops the detached input half.
        let _ = browser_port.shutdown(std::net::Shutdown::Both);
    }

    #[test]
    fn a_closed_browser_port_stops_the_session() {
        let (daemon_side, bridge_side) = UnixStream::pair().expect("pair");
        let sink = Sink::default();
        // The real daemon ends the connection when the browser closes the port.
        let daemon = std::thread::spawn(move || {
            let mut daemon_side = daemon_side;
            assert!(read_frame(&mut daemon_side).is_err());
        });
        let end = pump(Cursor::new(Vec::new()), sink.clone(), bridge_side);
        daemon.join().expect("daemon thread");
        assert_eq!(end, PumpEnd::BrowserClosed);
        // A closed stdin shuts the write half down and no frame is invented.
        assert!(sink.frames().is_empty());
    }

    #[test]
    fn a_missing_daemon_is_reported_as_a_typed_error_frame() {
        let sink = Sink::default();
        emit_unavailable(&mut sink.clone());
        let frames = sink.frames();
        assert_eq!(frames.len(), 1);
        let text = String::from_utf8_lossy(&frames[0]).into_owned();
        assert!(text.contains("daemon_unavailable"));
        assert!(text.contains("not running"));
        // The error envelope never carries a value or a diagnostic detail.
        assert!(!text.contains("password"));
        assert!(!text.contains("token"));
    }

    #[test]
    fn an_oversized_frame_never_crosses_the_bridge() {
        let (daemon_side, bridge_side) = UnixStream::pair().expect("pair");
        let big = vec![b'x'; MAX_FRAME_BYTES + 1];
        let daemon = std::thread::spawn(move || {
            let mut daemon_side = daemon_side;
            // The pump stops as soon as the frame exceeds the cap.
            assert!(read_frame(&mut daemon_side).is_err());
        });
        let end = pump(Cursor::new(big), Sink::default(), bridge_side);
        daemon.join().expect("daemon thread");
        assert_eq!(end, PumpEnd::BrowserClosed);
    }
}
