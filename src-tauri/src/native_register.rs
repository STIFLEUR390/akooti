//! Native Messaging host manifests, per browser and per operating system.
//!
//! The extension can only reach [`crate::native_host`] when a manifest written
//! in a browser-specific location declares the host name and the exact
//! extension origins allowed to run it. This module computes those paths and
//! the manifest document, then installs, checks, repairs or removes them.
//!
//! Nothing here reads the vault, a token or a secret: registration is a pure
//! file operation and every function is testable against a temporary
//! directory. Windows stores the mapping in the registry, which the deferred
//! named-pipe transport ([ADR 0006]) already excludes; the call reports the gap
//! instead of writing a manifest nobody would read.
//!
//! [ADR 0006]: ../../docs/adr/0006-transport-ipc-socket-unix.md

use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

/// Host name shared by every manifest and by the extension.
pub const HOST_NAME: &str = "org.akooti.host";

/// Firefox add-on id, pinned by `apps/extension/wxt.config.ts`.
pub const FIREFOX_EXTENSION_ID: &str = "extension@akooti.local";

/// Human description copied into every manifest.
const HOST_DESCRIPTION: &str =
    "Akooti browser bridge: relay between the extension and the local daemon";

/// Browsers that can host the bridge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Browser {
    /// Google Chrome.
    Chrome,
    /// Chromium.
    Chromium,
    /// Microsoft Edge.
    Edge,
    /// Brave.
    Brave,
    /// Mozilla Firefox.
    Firefox,
}

impl Browser {
    /// Every browser the registration command knows.
    pub const ALL: [Self; 5] = [
        Self::Chrome,
        Self::Chromium,
        Self::Edge,
        Self::Brave,
        Self::Firefox,
    ];

    /// Stable name used on the command line.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Chrome => "chrome",
            Self::Chromium => "chromium",
            Self::Edge => "edge",
            Self::Brave => "brave",
            Self::Firefox => "firefox",
        }
    }

    /// Parses a command-line name; accepts a few common aliases.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "chrome" | "google-chrome" => Some(Self::Chrome),
            "chromium" => Some(Self::Chromium),
            "edge" | "microsoft-edge" => Some(Self::Edge),
            "brave" | "brave-browser" => Some(Self::Brave),
            "firefox" | "mozilla" => Some(Self::Firefox),
            _ => None,
        }
    }

    /// Whether the manifest uses `allowed_extensions` instead of origins.
    #[must_use]
    pub const fn is_firefox(self) -> bool {
        matches!(self, Self::Firefox)
    }
}

/// Everything needed to describe one manifest.
#[derive(Debug, Clone)]
pub struct RegistrationRequest<'a> {
    /// Browser receiving the manifest.
    pub browser: Browser,
    /// User home directory (never the real one in tests).
    pub home: &'a Path,
    /// `XDG_CONFIG_HOME`, honoured on Linux only.
    pub config_home: Option<&'a Path>,
    /// Absolute path of the `akooti` binary the browser will spawn.
    pub host_path: &'a Path,
    /// Chromium extension id (`chrome://extensions`); required for Chromium.
    pub chromium_extension_id: Option<&'a str>,
    /// Firefox add-on id; defaults to [`FIREFOX_EXTENSION_ID`].
    pub firefox_extension_id: Option<&'a str>,
}

/// One manifest: where it lives and what it contains.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeHostManifest {
    /// Browser it belongs to.
    pub browser: Browser,
    /// Absolute manifest path.
    pub path: PathBuf,
    /// Manifest document exactly as written to disk.
    pub document: String,
}

/// State of a manifest on disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManifestState {
    /// No manifest at the computed path.
    Missing,
    /// The manifest is byte-identical to what this build writes.
    Current,
    /// A manifest exists but differs: another binary path or extension id.
    Outdated,
    /// A manifest exists but cannot be read.
    Unreadable,
}

/// Result of an install.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallOutcome {
    /// The manifest was created.
    Installed,
    /// An identical manifest was already there.
    Unchanged,
    /// A stale manifest was replaced.
    Repaired,
}

impl InstallOutcome {
    /// Stable name printed by the command.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Installed => "installed",
            Self::Unchanged => "unchanged",
            Self::Repaired => "repaired",
        }
    }
}

/// Why a registration could not be planned or applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegisterError {
    /// The platform stores the manifest in the registry (Windows).
    UnsupportedPlatform,
    /// A Chromium manifest needs the extension id.
    MissingExtensionId(Browser),
    /// The extension id is not shaped like one.
    InvalidExtensionId(Browser),
    /// The file system refused the operation.
    Io(String),
}

impl RegisterError {
    /// Message shown to the user, without any secret.
    #[must_use]
    pub fn message(&self) -> String {
        match self {
            Self::UnsupportedPlatform => {
                "native messaging registration is not supported on this platform yet \
                 (Windows uses the registry; see ADR 0006)"
                    .to_owned()
            }
            Self::MissingExtensionId(browser) => format!(
                "{} needs an extension id: pass --extension-id with the id shown by the browser",
                browser.as_str()
            ),
            Self::InvalidExtensionId(browser) => format!(
                "the extension id given for {} is not valid",
                browser.as_str()
            ),
            Self::Io(reason) => format!("native messaging registration failed: {reason}"),
        }
    }
}

/// Manifest payload; `allowed_origins` and `allowed_extensions` are exclusive.
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
struct ManifestDocument<'a> {
    name: &'a str,
    description: &'a str,
    path: String,
    #[serde(rename = "type")]
    kind: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    allowed_origins: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    allowed_extensions: Option<Vec<String>>,
}

/// Builds the manifest for `request`.
///
/// # Errors
///
/// Returns [`RegisterError::MissingExtensionId`] or
/// [`RegisterError::InvalidExtensionId`] when the origins cannot be expressed,
/// [`RegisterError::UnsupportedPlatform`] on Windows, and
/// [`RegisterError::Io`] when the host binary path is not absolute.
pub fn plan(request: &RegistrationRequest<'_>) -> Result<NativeHostManifest, RegisterError> {
    let host = request
        .host_path
        .to_str()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| RegisterError::Io("the host path is not valid UTF-8".to_owned()))?;
    if !request.host_path.is_absolute() {
        return Err(RegisterError::Io(
            "the host path must be absolute so the browser can spawn it".to_owned(),
        ));
    }
    let (allowed_origins, allowed_extensions) = if request.browser.is_firefox() {
        let id = request.firefox_extension_id.unwrap_or(FIREFOX_EXTENSION_ID);
        if !is_firefox_id(id) {
            return Err(RegisterError::InvalidExtensionId(request.browser));
        }
        (None, Some(vec![id.to_owned()]))
    } else {
        let id = request
            .chromium_extension_id
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or(RegisterError::MissingExtensionId(request.browser))?;
        if !is_chromium_id(id) {
            return Err(RegisterError::InvalidExtensionId(request.browser));
        }
        (Some(vec![format!("chrome-extension://{id}/")]), None)
    };
    let document = ManifestDocument {
        name: HOST_NAME,
        description: HOST_DESCRIPTION,
        path: host.to_owned(),
        kind: "stdio",
        allowed_origins,
        allowed_extensions,
    };
    let document = serde_json::to_string_pretty(&document)
        .map_err(|_| RegisterError::Io("the manifest could not be serialised".to_owned()))?;
    Ok(NativeHostManifest {
        browser: request.browser,
        path: manifest_path(request)?,
        document: format!("{document}\n"),
    })
}

/// Absolute manifest path for `request.browser`.
///
/// # Errors
///
/// Returns [`RegisterError::UnsupportedPlatform`] on Windows.
pub fn manifest_path(request: &RegistrationRequest<'_>) -> Result<PathBuf, RegisterError> {
    if cfg!(windows) {
        return Err(RegisterError::UnsupportedPlatform);
    }
    Ok(base_directory(request).join(format!("{HOST_NAME}.json")))
}

/// Directory a browser scans for Native Messaging manifests.
fn base_directory(request: &RegistrationRequest<'_>) -> PathBuf {
    let browser = request.browser;
    if cfg!(target_os = "macos") {
        let support = request.home.join("Library/Application Support");
        let vendor = match browser {
            Browser::Chrome => support.join("Google/Chrome"),
            Browser::Chromium => support.join("Chromium"),
            Browser::Edge => support.join("Microsoft Edge"),
            Browser::Brave => support.join("BraveSoftware/Brave-Browser"),
            Browser::Firefox => support.join("Mozilla"),
        };
        return vendor.join("NativeMessagingHosts");
    }
    if browser.is_firefox() {
        return request.home.join(".mozilla/native-messaging-hosts");
    }
    let config = request
        .config_home
        .map_or_else(|| request.home.join(".config"), Path::to_path_buf);
    let vendor = match browser {
        Browser::Chrome => "google-chrome",
        Browser::Chromium => "chromium",
        Browser::Edge => "microsoft-edge",
        Browser::Brave => "BraveSoftware/Brave-Browser",
        Browser::Firefox => ".mozilla",
    };
    config.join(vendor).join("NativeMessagingHosts")
}

/// Reads the current state of `manifest` without modifying anything.
#[must_use]
pub fn check(manifest: &NativeHostManifest) -> ManifestState {
    match fs::read_to_string(&manifest.path) {
        Ok(current) if current == manifest.document => ManifestState::Current,
        Ok(_) => ManifestState::Outdated,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => ManifestState::Missing,
        Err(_) => ManifestState::Unreadable,
    }
}

/// Writes `manifest`, creating parent directories as needed.
///
/// The operation is idempotent: an identical manifest is left untouched, and a
/// stale one is replaced in a single rename so a browser reading it
/// concurrently never sees a truncated document.
///
/// # Errors
///
/// Returns [`RegisterError::Io`] when a directory or the file cannot be written.
pub fn install(manifest: &NativeHostManifest) -> Result<InstallOutcome, RegisterError> {
    let state = check(manifest);
    match state {
        ManifestState::Current => return Ok(InstallOutcome::Unchanged),
        ManifestState::Missing | ManifestState::Outdated | ManifestState::Unreadable => {}
    }
    let parent = manifest
        .path
        .parent()
        .ok_or_else(|| RegisterError::Io("the manifest path has no parent".to_owned()))?;
    fs::create_dir_all(parent).map_err(io_error)?;
    let staged = manifest.path.with_extension("json.tmp");
    fs::write(&staged, &manifest.document).map_err(io_error)?;
    fs::rename(&staged, &manifest.path).map_err(io_error)?;
    Ok(match state {
        ManifestState::Missing => InstallOutcome::Installed,
        _ => InstallOutcome::Repaired,
    })
}

/// Removes `manifest` when present. Idempotent: a missing file is not an error.
///
/// # Errors
///
/// Returns [`RegisterError::Io`] when a manifest exists but cannot be removed.
pub fn uninstall(manifest: &NativeHostManifest) -> Result<bool, RegisterError> {
    match fs::remove_file(&manifest.path) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(io_error(error)),
    }
}

fn io_error(error: std::io::Error) -> RegisterError {
    RegisterError::Io(error.kind().to_string())
}

/// Chromium ids are 32 characters in `a`..`p`.
fn is_chromium_id(value: &str) -> bool {
    value.len() == 32 && value.chars().all(|c| ('a'..='p').contains(&c))
}

/// Firefox ids are an email-like string.
fn is_firefox_id(value: &str) -> bool {
    let mut parts = value.split('@');
    let (Some(local), Some(domain), None) = (parts.next(), parts.next(), parts.next()) else {
        return false;
    };
    !local.is_empty() && domain.contains('.') && !value.chars().any(char::is_whitespace)
}

#[cfg(test)]
mod tests {
    use super::{
        Browser, FIREFOX_EXTENSION_ID, HOST_NAME, InstallOutcome, ManifestState, RegisterError,
        RegistrationRequest, check, install, is_chromium_id, is_firefox_id, manifest_path, plan,
        uninstall,
    };
    use std::path::Path;

    const CHROMIUM_ID: &str = "abcdefghijklmnopabcdefghijklmnop";

    fn request<'a>(home: &'a Path, browser: Browser) -> RegistrationRequest<'a> {
        RegistrationRequest {
            browser,
            home,
            config_home: None,
            host_path: Path::new("/usr/local/bin/akooti"),
            chromium_extension_id: Some(CHROMIUM_ID),
            firefox_extension_id: None,
        }
    }

    #[test]
    fn chromium_manifests_declare_the_exact_extension_origin() {
        let home = Path::new("/home/tester");
        let manifest = plan(&request(home, Browser::Chrome)).expect("plan");
        assert_eq!(manifest.browser, Browser::Chrome);
        assert_eq!(
            manifest
                .path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned()),
            Some(format!("{HOST_NAME}.json"))
        );
        assert!(manifest.document.contains("\"name\": \"org.akooti.host\""));
        assert!(
            manifest
                .document
                .contains(&format!("chrome-extension://{CHROMIUM_ID}/"))
        );
        assert!(manifest.document.contains("\"type\": \"stdio\""));
        assert!(manifest.document.contains("/usr/local/bin/akooti"));
        assert!(!manifest.document.contains("allowed_extensions"));
    }

    #[test]
    fn firefox_manifests_use_allowed_extensions_and_the_pinned_id() {
        let home = Path::new("/home/tester");
        let manifest = plan(&request(home, Browser::Firefox)).expect("plan");
        assert!(manifest.document.contains("allowed_extensions"));
        assert!(manifest.document.contains(FIREFOX_EXTENSION_ID));
        assert!(!manifest.document.contains("allowed_origins"));
        assert!(
            manifest
                .path
                .ends_with(".mozilla/native-messaging-hosts/org.akooti.host.json")
        );
    }

    #[test]
    fn every_browser_maps_to_its_own_directory() {
        let home = Path::new("/home/tester");
        let chrome = manifest_path(&request(home, Browser::Chrome)).expect("path");
        let chromium = manifest_path(&request(home, Browser::Chromium)).expect("path");
        let edge = manifest_path(&request(home, Browser::Edge)).expect("path");
        let brave = manifest_path(&request(home, Browser::Brave)).expect("path");
        assert_ne!(chrome, chromium);
        assert_ne!(chrome, edge);
        assert_ne!(chrome, brave);
        assert!(chrome.starts_with("/home/tester/.config/google-chrome/NativeMessagingHosts"));
        assert!(brave.starts_with("/home/tester/.config/BraveSoftware/Brave-Browser"));
    }

    #[test]
    fn a_xdg_config_home_is_honoured_for_chromium_browsers_only() {
        let home = Path::new("/home/tester");
        let config = Path::new("/tmp/xdg");
        let mut request = request(home, Browser::Chrome);
        request.config_home = Some(config);
        let chrome = plan(&request).expect("plan");
        assert!(chrome.path.starts_with("/tmp/xdg/google-chrome"));
        let mut firefox_request = request;
        firefox_request.browser = Browser::Firefox;
        let firefox = plan(&firefox_request).expect("plan");
        assert!(firefox.path.starts_with("/home/tester/.mozilla"));
    }

    #[test]
    fn a_missing_or_invalid_extension_id_is_refused_before_any_write() {
        let home = Path::new("/home/tester");
        let mut request = request(home, Browser::Chrome);
        request.chromium_extension_id = None;
        assert_eq!(
            plan(&request),
            Err(RegisterError::MissingExtensionId(Browser::Chrome))
        );
        request.chromium_extension_id = Some("not-an-id");
        assert_eq!(
            plan(&request),
            Err(RegisterError::InvalidExtensionId(Browser::Chrome))
        );
    }

    #[test]
    fn a_relative_host_path_is_refused() {
        let home = Path::new("/home/tester");
        let mut request = request(home, Browser::Chrome);
        request.host_path = Path::new("target/debug/akooti");
        assert!(matches!(plan(&request), Err(RegisterError::Io(_))));
    }

    #[test]
    fn install_is_idempotent_and_repairs_a_stale_manifest() {
        let directory = tempfile::tempdir().expect("tempdir");
        let home = directory.path();
        let manifest = plan(&request(home, Browser::Chrome)).expect("plan");
        assert_eq!(check(&manifest), ManifestState::Missing);

        assert_eq!(install(&manifest), Ok(InstallOutcome::Installed));
        assert_eq!(check(&manifest), ManifestState::Current);
        assert_eq!(install(&manifest), Ok(InstallOutcome::Unchanged));

        // A manifest left by an older build points at another binary.
        std::fs::write(
            &manifest.path,
            manifest
                .document
                .replace("/usr/local/bin/akooti", "/opt/old/akooti"),
        )
        .expect("seed stale manifest");
        assert_eq!(check(&manifest), ManifestState::Outdated);
        assert_eq!(install(&manifest), Ok(InstallOutcome::Repaired));
        let written = std::fs::read_to_string(&manifest.path).expect("read");
        assert_eq!(written, manifest.document);
        assert!(!manifest.path.with_extension("json.tmp").exists());
    }

    #[test]
    fn uninstall_removes_the_manifest_once_and_stays_idempotent() {
        let directory = tempfile::tempdir().expect("tempdir");
        let manifest = plan(&request(directory.path(), Browser::Brave)).expect("plan");
        install(&manifest).expect("install");
        assert_eq!(uninstall(&manifest), Ok(true));
        assert_eq!(check(&manifest), ManifestState::Missing);
        assert_eq!(uninstall(&manifest), Ok(false));
    }

    #[test]
    fn extension_id_shapes_are_validated() {
        assert!(is_chromium_id(CHROMIUM_ID));
        assert!(!is_chromium_id("ABCDEFGHIJKLMNOPABCDEFGHIJKLMNOP"));
        assert!(!is_chromium_id("short"));
        assert!(is_firefox_id(FIREFOX_EXTENSION_ID));
        assert!(!is_firefox_id("no-at-sign"));
        assert!(!is_firefox_id("a@b@c.local"));
        assert!(!is_firefox_id("with space@akooti.local"));
    }

    #[test]
    fn browser_names_round_trip() {
        for browser in Browser::ALL {
            assert_eq!(Browser::parse(browser.as_str()), Some(browser));
        }
        assert_eq!(Browser::parse("Brave-Browser"), Some(Browser::Brave));
        assert_eq!(Browser::parse("safari"), None);
    }
}
