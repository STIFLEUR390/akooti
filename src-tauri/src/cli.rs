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
    /// Native Messaging host manifest registration.
    NativeRegister(NativeRegisterOptions),
    /// Print usage and exit without starting anything.
    Help,
    /// Print the version and exit without starting anything.
    Version,
}

/// Options of `akooti native-register`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NativeRegisterOptions {
    /// Browsers to touch, as typed; empty means every known browser.
    pub browsers: Vec<String>,
    /// Chromium extension id shown by the browser.
    pub extension_id: Option<String>,
    /// Firefox add-on id, overriding the pinned one.
    pub firefox_id: Option<String>,
    /// Absolute path of the host binary; defaults to the running executable.
    pub host_path: Option<String>,
    /// Home directory, so the command can be scripted against a sandbox.
    pub home: Option<String>,
    /// Remove the manifests instead of writing them.
    pub uninstall: bool,
    /// Report the current state without writing anything.
    pub check: bool,
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
        "native-register" | "native_register" => {
            CliMode::NativeRegister(parse_native_register(arguments.collect()))
        }
        _ => CliMode::Gui,
    }
}

/// Parses the flags of `akooti native-register`.
fn parse_native_register(arguments: Vec<String>) -> NativeRegisterOptions {
    let mut options = NativeRegisterOptions::default();
    let mut iterator = arguments.into_iter();
    while let Some(argument) = iterator.next() {
        let (flag, inline) = match argument.split_once('=') {
            Some((flag, value)) => (flag.to_owned(), Some(value.to_owned())),
            None => (argument, None),
        };
        match flag.as_str() {
            "--browser" | "-b" => {
                if let Some(value) = positional(inline, &mut iterator) {
                    options
                        .browsers
                        .extend(value.split(',').map(|name| name.trim().to_owned()));
                }
            }
            "--extension-id" | "--extension_id" => {
                options.extension_id = positional(inline, &mut iterator);
            }
            "--firefox-id" | "--firefox_id" => {
                options.firefox_id = positional(inline, &mut iterator);
            }
            "--host" | "--host-path" => options.host_path = positional(inline, &mut iterator),
            "--home" => options.home = positional(inline, &mut iterator),
            "--uninstall" => options.uninstall = true,
            "--check" => options.check = true,
            _ => {}
        }
    }
    options.browsers.retain(|name| !name.is_empty());
    if !options.browsers.is_empty() {
        options.browsers.sort();
        options.browsers.dedup();
    }
    options
}

/// Value of a flag: inline (`--flag=value`) or the next argument.
fn positional(
    inline: Option<String>,
    iterator: &mut impl Iterator<Item = String>,
) -> Option<String> {
    inline.or_else(|| iterator.next())
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
    akooti                        Lance l'interface graphique et le daemon
    akooti mcp --token <TOKEN>    Serveur MCP headless sur stdio
    akooti native-host            Pont Native Messaging pour l'extension
    akooti native-register [...]  Installe les manifestes Native Messaging
    akooti help                   Affiche cette aide
    akooti version                Affiche la version

OPTIONS:
    -t, --token <TOKEN>       Jeton MCP cloisonné, généré par l'application
    -h, --help                Affiche cette aide
    -V, --version             Affiche la version

NATIVE-REGISTER:
    -b, --browser <NAME>      chrome, chromium, edge, brave, firefox (défaut : tous)
        --extension-id <ID>   Identifiant de l'extension Chromium (obligatoire)
        --firefox-id <ID>     Identifiant du module Firefox (défaut : extension@akooti.local)
        --host <PATH>         Chemin absolu du binaire (défaut : binaire courant)
        --home <PATH>         Racine utilisateur à utiliser (installation scriptée)
        --check               Vérifie les manifestes sans rien écrire
        --uninstall           Supprime les manifestes
";

#[cfg(test)]
mod tests {
    use super::{CliMode, NativeRegisterOptions, parse_mode};

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

    #[test]
    fn native_register_reads_every_flag_in_both_spellings() {
        assert_eq!(
            parse_mode([
                "akooti",
                "native-register",
                "--browser",
                "chrome,brave",
                "--extension-id=abcdefghijklmnopabcdefghijklmnop",
                "--host",
                "/usr/bin/akooti",
                "--check",
            ]),
            CliMode::NativeRegister(NativeRegisterOptions {
                browsers: vec!["brave".to_owned(), "chrome".to_owned()],
                extension_id: Some("abcdefghijklmnopabcdefghijklmnop".to_owned()),
                firefox_id: None,
                host_path: Some("/usr/bin/akooti".to_owned()),
                home: None,
                uninstall: false,
                check: true,
            })
        );
        assert_eq!(
            parse_mode(["akooti", "native_register", "-b", "firefox", "--uninstall"]),
            CliMode::NativeRegister(NativeRegisterOptions {
                browsers: vec!["firefox".to_owned()],
                uninstall: true,
                ..NativeRegisterOptions::default()
            })
        );
    }

    #[test]
    fn native_register_defaults_to_every_browser() {
        assert_eq!(
            parse_mode(["akooti", "native-register"]),
            CliMode::NativeRegister(NativeRegisterOptions::default())
        );
    }
}
