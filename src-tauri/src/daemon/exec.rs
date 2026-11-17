//! Child-process execution with secret injection (`vault_exec`).
//!
//! Three rules (`docs/fonctionnalites.md` §3, `docs/regles.md` §1.1):
//!
//! * secrets only reach the child through its **environment**, never `argv`;
//! * captured output is rewritten so every injected value becomes `***`;
//! * the capture happens in memory — no temporary file holds the output.

use std::io::Read;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use akooti_core::SecretString;
use akooti_ipc::ExecOutcome;

/// Default wall-clock budget.
pub const DEFAULT_TIMEOUT_SECONDS: u64 = 30;
/// Maximum captured size per stream, to bound memory use.
const MAX_CAPTURE_BYTES: usize = 256 * 1024;

/// Runs `argv` with `variables` in its environment and masks the values.
///
/// # Errors
///
/// Returns a non-revealing message when the program cannot be spawned.
pub fn run(
    argv: &[String],
    variables: &[(String, SecretString)],
    timeout_seconds: Option<u64>,
) -> Result<ExecOutcome, String> {
    let program = argv.first().ok_or_else(|| "empty command".to_owned())?;
    let mut command = Command::new(program);
    command
        .args(&argv[1..])
        .env_clear()
        .envs(std::env::vars().filter(|(name, _)| !name.starts_with("AKOOTI_")))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (name, value) in variables {
        command.env(name, value.expose());
    }

    let started = Instant::now();
    let mut child = command
        .spawn()
        .map_err(|_| "the command could not be started".to_owned())?;

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let stdout_reader = stdout.map(|pipe| {
        thread::spawn(move || {
            let mut buffer = Vec::new();
            let _ = pipe.take(MAX_CAPTURE_BYTES as u64).read_to_end(&mut buffer);
            buffer
        })
    });
    let stderr_reader = stderr.map(|pipe| {
        thread::spawn(move || {
            let mut buffer = Vec::new();
            let _ = pipe.take(MAX_CAPTURE_BYTES as u64).read_to_end(&mut buffer);
            buffer
        })
    });

    let budget = Duration::from_secs(
        timeout_seconds
            .unwrap_or(DEFAULT_TIMEOUT_SECONDS)
            .clamp(1, 600),
    );
    let mut timed_out = false;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) => {
                if started.elapsed() >= budget {
                    let _ = child.kill();
                    let _ = child.wait();
                    timed_out = true;
                    break None;
                }
                thread::sleep(Duration::from_millis(25));
            }
            Err(_) => break None,
        }
    };

    let mut collected_stdout = join(stdout_reader);
    let mut collected_stderr = join(stderr_reader);
    for (_, value) in variables {
        collected_stdout = mask(collected_stdout, value.expose());
        collected_stderr = mask(collected_stderr, value.expose());
    }

    #[cfg(unix)]
    let signal = status.and_then(|status| {
        use std::os::unix::process::ExitStatusExt;
        status.signal().map(|number| number.to_string())
    });
    #[cfg(not(unix))]
    let signal: Option<String> = None;

    Ok(ExecOutcome {
        exit_code: status.and_then(|status| status.code()),
        signal,
        timed_out,
        stdout: collected_stdout,
        stderr: collected_stderr,
        duration_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
    })
}

fn join(handle: Option<thread::JoinHandle<Vec<u8>>>) -> String {
    handle
        .and_then(|handle| handle.join().ok())
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
        .unwrap_or_default()
}

/// Replaces every occurrence of `secret` with `***`, including its URL-encoded
/// and JSON-escaped forms, so a program that echoes its input cannot leak it.
fn mask(mut text: String, secret: &str) -> String {
    if secret.is_empty() {
        return text;
    }
    text = text.replace(secret, "***");
    let encoded = url_encode(secret);
    if encoded != secret {
        text = text.replace(&encoded, "***");
    }
    let escaped = serde_json::to_string(secret).unwrap_or_default();
    let escaped = escaped.trim_matches('"').to_owned();
    if escaped != secret {
        text = text.replace(&escaped, "***");
    }
    text
}

fn url_encode(secret: &str) -> String {
    let mut encoded = String::with_capacity(secret.len());
    for byte in secret.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(char::from(byte));
            }
            other => encoded.push_str(&format!("%{other:02X}")),
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::{mask, run};
    use akooti_core::SecretString;
    use akooti_ipc::ExecOutcome;

    fn secret(value: &str) -> SecretString {
        SecretString::new(value.to_owned())
    }

    #[test]
    fn masks_plain_url_encoded_and_escaped_forms() {
        let masked = mask("value=a b&x=a%20b&y=a b".to_owned(), "a b");
        assert!(!masked.contains("a b"));
        assert!(!masked.contains("a%20b"));
        assert!(masked.contains("***"));
    }

    #[test]
    fn running_a_program_exports_the_environment_and_masks_stdout() {
        #[cfg(unix)]
        let argv = vec![
            "sh".to_owned(),
            "-c".to_owned(),
            "printf '%s' \"$AKOOTI_PASSWORD\"".to_owned(),
        ];
        #[cfg(not(unix))]
        let argv = vec![
            "cmd".to_owned(),
            "/C".to_owned(),
            "echo %AKOOTI_PASSWORD%".to_owned(),
        ];

        let outcome: ExecOutcome = run(
            &argv,
            &[("AKOOTI_PASSWORD".to_owned(), secret("s3cret-value"))],
            Some(10),
        )
        .expect("exec");
        assert_eq!(outcome.exit_code, Some(0));
        assert!(!outcome.timed_out);
        assert!(!outcome.stdout.contains("s3cret-value"));
        assert!(outcome.stdout.contains("***"));
    }

    #[test]
    fn a_missing_program_is_reported_without_echoing_argv() {
        let error =
            run(&["akooti-does-not-exist".to_owned()], &[], Some(5)).expect_err("must fail");
        assert!(!error.contains("akooti-does-not-exist"));
    }

    #[test]
    fn an_empty_command_is_refused() {
        assert!(run(&[], &[], None).is_err());
    }
}
