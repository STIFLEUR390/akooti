//! Command-line surface of the single `akooti` binary.
//!
//! The mode is resolved **before** the Tauri builder runs so that headless
//! modes (`mcp`, `native-host`) never create a window, a tray icon, or a
//! webview process. Only the parsed mode and the optional token leave this
//! module; no secret is ever read here.
//!
//! CLI argument names mirror the `plugins.cli` block of `tauri.conf.json`:
//! `--token` on the `mcp` subcommand.

use std::ffi::OsString;

/// Subcommand selected on the command line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliMode {
    /// Default mode: graphical desktop application plus background daemon.
    Gui,
    /// Headless MCP server bound to a scoped token.
    Mcp { token: Option<String> },
    /// Native Messaging bridge for the browser extension.
    NativeHost,
    /// Print usage and exit without starting anything.
    Help,
    /// Print the version and exit without starting anything.
    Version,
}

/// Parses the raw process arguments into a [`CliMode`].
///
/// `args` must include the executable name at index 0, exactly like
/// [`std::env::args_os`]. Unknown arguments fall back to [`CliMode::Gui`] so a
/// desktop launcher that appends its own flags still opens the application.
pub fn parse_mode<I, S>(args: I) -> CliMode
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
{
    let mut arguments = args
        .into_iter()
        .map(Into::into)
        .skip(1)
        .map(|argument| argument.to_string_lossy().into_owned());
    let Some(first) = arguments.next() else {
        return CliMode::Gui;
    };
    match first.as_str() {
        "help" | "--help" | "-h" => CliMode::Help,
        "version" | "--version" | "-V" => CliMode::Version,
        "mcp" => CliMode::Mcp {
            token: parse_token(arguments),
        },
        "native-host" | "native_host" => CliMode::NativeHost,
        _ => CliMode::Gui,
    }
}

fn parse_token<I>(mut arguments: I) -> Option<String>
where
    I: Iterator<Item = String>,
{
    let mut token = None;
    while let Some(argument) = arguments.next() {
        if let Some(value) = argument.strip_prefix("--token=") {
            token = Some(value.to_owned());
            continue;
        }
        if argument == "--token" || argument == "-t" {
            token = arguments.next();
            continue;
        }
        // Positional token: `akooti mcp ak_mcp_…`
        if token.is_none() && !argument.starts_with('-') {
            token = Some(argument);
        }
    }
    token.filter(|value| !value.is_empty())
}

/// Usage text for `akooti help`.
pub const USAGE: &str = "\
Akooti — coffre-fort local-first, zero-knowledge et zero-leak

USAGE:
    akooti                      Lance l'interface graphique et le daemon
    akooti mcp --token <TOKEN>  Serveur MCP headless sur stdio
    akooti native-host          Pont Native Messaging pour l'extension
    akooti help                 Affiche cette aide
    akooti version              Affiche la version

OPTIONS:
    -t, --token <TOKEN>  Jeton MCP cloisonné, généré par l'application
    -h, --help           Affiche cette aide
    -V, --version        Affiche la version
";

#[cfg(test)]
mod tests {
    use super::{CliMode, parse_mode};

    #[test]
    fn bare_invocation_opens_the_desktop_app() {
        assert_eq!(parse_mode(["akooti"]), CliMode::Gui);
        assert_eq!(parse_mode(Vec::<String>::new()), CliMode::Gui);
        assert_eq!(parse_mode(["akooti", "--unknown-flag"]), CliMode::Gui);
    }

    #[test]
    fn mcp_mode_reads_the_token_in_both_spellings() {
        assert_eq!(
            parse_mode(["akooti", "mcp", "--token", "ak_mcp_123"]),
            CliMode::Mcp {
                token: Some("ak_mcp_123".to_owned())
            }
        );
        assert_eq!(
            parse_mode(["akooti", "mcp", "--token=ak_mcp_456"]),
            CliMode::Mcp {
                token: Some("ak_mcp_456".to_owned())
            }
        );
        assert_eq!(parse_mode(["akooti", "mcp"]), CliMode::Mcp { token: None });
    }

    #[test]
    fn help_and_version_short_circuit() {
        assert_eq!(parse_mode(["akooti", "--help"]), CliMode::Help);
        assert_eq!(parse_mode(["akooti", "-h"]), CliMode::Help);
        assert_eq!(parse_mode(["akooti", "help"]), CliMode::Help);
        assert_eq!(parse_mode(["akooti", "--version"]), CliMode::Version);
        assert_eq!(parse_mode(["akooti", "version"]), CliMode::Version);
        assert_eq!(parse_mode(["akooti", "native_host"]), CliMode::NativeHost);
    }
}
