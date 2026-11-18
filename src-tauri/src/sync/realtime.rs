//! Supabase Realtime subscription.
//!
//! The socket only tells us *that* something changed: the payload is never read
//! as data, and the actual delta comes from the REST pull. Reconnection uses an
//! exponential backoff and always triggers a pull, so a change missed while the
//! socket was down is not lost.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use serde_json::json;
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{Message, WebSocket};

/// Connection parameters for one subscription.
#[derive(Debug, Clone)]
pub struct RealtimeConfig {
    /// Supabase project URL (`https://…`).
    pub base_url: String,
    /// Public anon key.
    pub anon_key: String,
    /// Bearer token of the signed-in user.
    pub access_token: String,
    /// Supabase user identifier, used as the change filter.
    pub user_id: String,
}

/// Runs the subscription until `stop` is set.
///
/// `on_change` is called after a change event and after every successful
/// reconnection (the caller then pulls the delta).
pub fn run(config: &RealtimeConfig, stop: &AtomicBool, on_change: &dyn Fn()) {
    let mut backoff = 1_u64;
    while !stop.load(Ordering::Relaxed) {
        if listen(config, stop, on_change).is_ok() {
            backoff = 1;
        }
        if stop.load(Ordering::Relaxed) {
            break;
        }
        // The delta may have moved while the socket was down.
        on_change();
        sleep(Duration::from_secs(backoff));
        backoff = (backoff * 2).min(60);
    }
}

fn listen(config: &RealtimeConfig, stop: &AtomicBool, on_change: &dyn Fn()) -> Result<(), ()> {
    let url = websocket_url(config)?;
    let (mut socket, _) = tungstenite::connect(url).map_err(|_| ())?;
    set_read_timeout(&socket, Duration::from_secs(5));
    join(&mut socket, config)?;
    on_change();

    let mut last_heartbeat = Instant::now();
    loop {
        if stop.load(Ordering::Relaxed) {
            let _ = socket.close(None);
            return Ok(());
        }
        if last_heartbeat.elapsed() >= Duration::from_secs(25) {
            let heartbeat =
                json!({"topic": "phoenix", "event": "heartbeat", "payload": {}, "ref": "hb"});
            socket
                .send(Message::Text(heartbeat.to_string().into()))
                .map_err(|_| ())?;
            last_heartbeat = Instant::now();
        }
        match socket.read() {
            Ok(Message::Text(text)) => {
                if text.contains("\"event\":\"postgres_changes\"") {
                    on_change();
                }
            }
            Ok(Message::Ping(_) | Message::Pong(_) | Message::Binary(_) | Message::Frame(_)) => {
                let _ = socket.flush();
            }
            Ok(Message::Close(_)) => return Err(()),
            Err(tungstenite::Error::Io(error))
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                // Read timeout: loop again so heartbeat and stop stay responsive.
            }
            Err(_) => return Err(()),
        }
    }
}

fn websocket_url(config: &RealtimeConfig) -> Result<String, ()> {
    let base = config.base_url.trim_end_matches('/');
    let host = base
        .strip_prefix("https://")
        .map(|rest| format!("wss://{rest}"))
        .or_else(|| {
            base.strip_prefix("http://")
                .map(|rest| format!("ws://{rest}"))
        })
        .ok_or(())?;
    Ok(format!(
        "{host}/realtime/v1/websocket?apikey={}&vsn=1.0.0",
        super::time::encode_query_value(&config.anon_key)
    ))
}

fn join(
    socket: &mut WebSocket<MaybeTlsStream<std::net::TcpStream>>,
    config: &RealtimeConfig,
) -> Result<(), ()> {
    let join = json!({
        "topic": "realtime:public:akooti_sync_records",
        "event": "phx_join",
        "payload": {
            "access_token": config.access_token,
            "config": {
                "broadcast": {"self": false},
                "presence": {"key": ""},
                "postgres_changes": [{
                    "event": "*",
                    "schema": "public",
                    "table": "akooti_sync_records",
                    "filter": format!("user_id=eq.{}", config.user_id),
                }],
            },
        },
        "ref": "1",
    });
    socket
        .send(Message::Text(join.to_string().into()))
        .map_err(|_| ())
}

fn set_read_timeout(socket: &WebSocket<MaybeTlsStream<std::net::TcpStream>>, timeout: Duration) {
    match socket.get_ref() {
        MaybeTlsStream::Plain(stream) => {
            let _ = stream.set_read_timeout(Some(timeout));
        }
        MaybeTlsStream::Rustls(stream) => {
            let _ = stream.sock.set_read_timeout(Some(timeout));
        }
        _ => {}
    }
}

fn sleep(duration: Duration) {
    std::thread::sleep(duration);
}

#[cfg(test)]
mod tests {
    use super::websocket_url;
    use crate::sync::realtime::RealtimeConfig;

    fn config(base_url: &str) -> RealtimeConfig {
        RealtimeConfig {
            base_url: base_url.to_owned(),
            anon_key: "anon/key".to_owned(),
            access_token: "token".to_owned(),
            user_id: "user".to_owned(),
        }
    }

    #[test]
    fn https_becomes_wss_and_the_key_is_encoded() {
        let url = websocket_url(&config("https://project.supabase.co/")).expect("url");
        assert_eq!(
            url,
            "wss://project.supabase.co/realtime/v1/websocket?apikey=anon%2Fkey&vsn=1.0.0"
        );
    }

    #[test]
    fn a_non_http_url_is_refused() {
        assert!(websocket_url(&config("ftp://example.test")).is_err());
    }
}
