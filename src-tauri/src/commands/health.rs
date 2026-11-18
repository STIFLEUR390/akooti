//! Vault health commands: local audit, HIBP k-anonymity check, report export.
//!
//! The network lives here, never in `akooti-core` (rule SEC-5). Only the
//! 5-character SHA-1 prefix leaves the machine; the suffixes returned by HIBP
//! are compared locally by the core. The lookup is never automatic: the UI
//! calls [`check_breaches`] from an explicit user gesture.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use akooti_core::{HealthFinding, HealthReport};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

use crate::error::CommandError;
use crate::vault_runtime::VaultRuntime;

/// Event carrying `{ phase, done, total }` during an audit or a breach check.
pub const PROGRESS_EVENT: &str = "health:progress";

const HIBP_RANGE_URL: &str = "https://api.pwnedpasswords.com/range/";
const HIBP_TIMEOUT_SECONDS: u64 = 5;
const HIBP_MAX_BODY_BYTES: u64 = 512 * 1024;
const HIBP_MIN_INTERVAL: Duration = Duration::from_millis(150);
const HIBP_CACHE_TTL: Duration = Duration::from_secs(24 * 60 * 60);
const USER_AGENT: &str = concat!("akooti/", env!("CARGO_PKG_VERSION"));

/// One audited item, safe to send to the webview: no secret, no digest.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthFindingDto {
    /// Item identifier, used by the dashboard to open the entry.
    pub item_id: String,
    /// Display title.
    pub title: String,
    /// Primary URL, when set.
    pub primary_url: Option<String>,
    /// Username, when set.
    pub username: Option<String>,
    /// zxcvbn score, 0-4.
    pub score: u8,
    /// Another live item stores the same password.
    pub reused: bool,
    /// The password appeared in a known breach.
    pub compromised: bool,
    /// Days since the last password change.
    pub age_days: Option<i64>,
}

impl From<HealthFinding> for HealthFindingDto {
    fn from(finding: HealthFinding) -> Self {
        Self {
            item_id: finding.item_id,
            title: finding.title,
            primary_url: finding.primary_url,
            username: finding.username,
            score: finding.score,
            reused: finding.reused,
            compromised: finding.compromised,
            age_days: finding.age_days,
        }
    }
}

/// Local audit result, as displayed by the dashboard.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthReportDto {
    /// One finding per item holding a password.
    pub findings: Vec<HealthFindingDto>,
    /// Live items without a password.
    pub without_password: usize,
    /// Mean zxcvbn score, when anything was scored.
    pub average_score: Option<f64>,
}

impl From<HealthReport> for HealthReportDto {
    fn from(report: HealthReport) -> Self {
        Self {
            findings: report.findings.into_iter().map(Into::into).collect(),
            without_password: report.without_password,
            average_score: report.average_score,
        }
    }
}

impl HealthReportDto {
    /// CSV export of the report. Contains no secret, by construction.
    fn to_csv(&self) -> String {
        let mut output =
            String::from("titre,url,identifiant,score,reutilise,compromis,anciennete_jours\n");
        for finding in &self.findings {
            output.push_str(&csv_field(&finding.title));
            output.push(',');
            output.push_str(&csv_field(finding.primary_url.as_deref().unwrap_or("")));
            output.push(',');
            output.push_str(&csv_field(finding.username.as_deref().unwrap_or("")));
            output.push(',');
            output.push_str(&finding.score.to_string());
            output.push(',');
            output.push_str(if finding.reused { "oui" } else { "non" });
            output.push(',');
            output.push_str(if finding.compromised { "oui" } else { "non" });
            output.push(',');
            output.push_str(
                &finding
                    .age_days
                    .map_or_else(String::new, |days| days.to_string()),
            );
            output.push('\n');
        }
        output
    }
}

/// Outcome of a HIBP check.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BreachReportDto {
    /// Items whose prefix was checked.
    pub checked: usize,
    /// Items now flagged as compromised.
    pub compromised: usize,
    /// Items left unchecked because the service did not answer.
    pub unknown: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProgressDto {
    phase: &'static str,
    done: usize,
    total: usize,
}

/// Scores every stored password locally and persists strength and age data.
///
/// Runs on a worker thread (`command(async)`) so the webview stays responsive;
/// progress is emitted on [`PROGRESS_EVENT`].
///
/// # Errors
///
/// Returns `locked` when the vault is locked, or an item/decryption error.
#[tauri::command(async)]
pub fn audit_health(
    app: AppHandle,
    runtime: State<'_, Arc<VaultRuntime>>,
) -> Result<HealthReportDto, CommandError> {
    runtime.touch();
    let now = now_seconds();
    let report = runtime.with_vault(|vault| {
        let mut last_reported = 0;
        vault.audit_health(now, |done, total| {
            if done == total || done - last_reported >= 10 {
                last_reported = done;
                emit_progress(&app, "local", done, total);
            }
        })
    })?;
    Ok(report.into())
}

/// Checks the stored passwords against HIBP via k-anonymity.
///
/// # Errors
///
/// Returns `locked` when the vault is locked, or a database error when a flag
/// cannot be persisted. Network failures are reported as `unknown`, never as an
/// error: an offline machine must not block the audit.
#[tauri::command(async)]
pub fn check_breaches(
    app: AppHandle,
    runtime: State<'_, Arc<VaultRuntime>>,
) -> Result<BreachReportDto, CommandError> {
    runtime.touch();
    let targets = runtime.with_vault(akooti_core::Vault::breach_targets)?;
    let total = targets.len();
    let mut report = BreachReportDto {
        checked: 0,
        compromised: 0,
        unknown: 0,
    };
    for (index, target) in targets.iter().enumerate() {
        match range_suffixes(&target.prefix) {
            Ok(suffixes) => {
                let compromised = runtime
                    .with_vault(|vault| vault.apply_breach_range(&target.prefix, &suffixes))?;
                report.checked += target.items;
                report.compromised += compromised;
            }
            Err(()) => report.unknown += target.items,
        }
        emit_progress(&app, "breach", index + 1, total);
    }
    Ok(report)
}

/// Writes the report to a CSV file chosen by the user.
///
/// # Errors
///
/// Returns `dialog` when the picker cannot be shown or the path is unusable, or
/// `io` when the file cannot be written.
#[tauri::command(async)]
pub fn export_health_report(
    app: AppHandle,
    report: HealthReportDto,
) -> Result<Option<String>, CommandError> {
    use tauri_plugin_dialog::DialogExt;

    let picked = app
        .dialog()
        .file()
        .set_title("Enregistrer le rapport de santé")
        .set_file_name("akooti-sante.csv")
        .add_filter("CSV", &["csv"])
        .blocking_save_file();
    let Some(path) = picked else {
        return Ok(None);
    };
    let path = path
        .into_path()
        .map_err(|_| CommandError::new("dialog", "Ce fichier n'est pas accessible."))?;
    std::fs::write(&path, report.to_csv())
        .map_err(|_| CommandError::new("io", "Le rapport n'a pas pu être écrit."))?;
    Ok(Some(path.to_string_lossy().into_owned()))
}

fn emit_progress(app: &AppHandle, phase: &'static str, done: usize, total: usize) {
    let _ = app.emit(PROGRESS_EVENT, ProgressDto { phase, done, total });
}

fn now_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| {
            i64::try_from(elapsed.as_secs()).unwrap_or(i64::MAX)
        })
}

fn range_suffixes(prefix: &str) -> Result<HashSet<String>, ()> {
    if let Some(suffixes) = cached(prefix) {
        return Ok(suffixes);
    }
    // Politeness: HIBP is a free service, one request at a time is enough.
    std::thread::sleep(HIBP_MIN_INTERVAL);
    let suffixes = fetch_range(prefix)?;
    if let Ok(mut cache) = cache().lock() {
        cache.insert(prefix.to_owned(), (Instant::now(), suffixes.clone()));
    }
    Ok(suffixes)
}

/// HIBP range responses kept in memory, keyed by prefix, with their fetch time.
type RangeCache = Mutex<HashMap<String, (Instant, HashSet<String>)>>;

fn cache() -> &'static RangeCache {
    static CACHE: OnceLock<RangeCache> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn cached(prefix: &str) -> Option<HashSet<String>> {
    let cache = cache().lock().ok()?;
    let (fetched_at, suffixes) = cache.get(prefix)?;
    (fetched_at.elapsed() < HIBP_CACHE_TTL).then(|| suffixes.clone())
}

fn fetch_range(prefix: &str) -> Result<HashSet<String>, ()> {
    if prefix.len() != akooti_core::SHA1_PREFIX_LEN
        || !prefix.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(());
    }
    let url = format!("{HIBP_RANGE_URL}{prefix}");
    let mut response = ureq::get(&url)
        .header("Add-Padding", "true")
        .header("User-Agent", USER_AGENT)
        .config()
        .timeout_global(Some(Duration::from_secs(HIBP_TIMEOUT_SECONDS)))
        .build()
        .call()
        .map_err(|_| ())?;
    let body = response
        .body_mut()
        .with_config()
        .limit(HIBP_MAX_BODY_BYTES)
        .read_to_string()
        .map_err(|_| ())?;
    Ok(parse_range_body(&body))
}

/// Parses a HIBP range body (`SUFFIX:COUNT` lines, CRLF) into suffixes.
///
/// Padded entries carry a zero count and are dropped: they exist to hide the
/// real traffic, never to flag a password.
fn parse_range_body(body: &str) -> HashSet<String> {
    body.lines()
        .filter_map(|line| {
            let (suffix, count) = line.trim().split_once(':')?;
            let count: u32 = count.trim().parse().ok()?;
            let suffix = suffix.trim();
            if count == 0 || suffix.len() != 35 {
                return None;
            }
            Some(suffix.to_ascii_uppercase())
        })
        .collect()
}

fn csv_field(value: &str) -> String {
    if value.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::{HealthReportDto, parse_range_body};
    use akooti_core::{HealthFinding, HealthReport};

    #[test]
    fn padded_and_malformed_range_lines_are_ignored() {
        let repeated = "A".repeat(35);
        let body = format!(
            concat!(
                "0018A45C4D1DEF81644B54AB7F969B88D65:12\r\n",
                "00D4F6E8FA6EECAD2A3AA415EEC418D38EC:0\r\n",
                "not-a-line\r\n",
                "SHORT:5\r\n",
                "{}:3\r\n",
            ),
            repeated
        );
        let suffixes = parse_range_body(&body);
        assert_eq!(suffixes.len(), 2);
        assert!(suffixes.contains("0018A45C4D1DEF81644B54AB7F969B88D65"));
        assert!(suffixes.contains(&repeated));
        assert!(!suffixes.contains("00D4F6E8FA6EECAD2A3AA415EEC418D38EC"));
    }

    #[test]
    fn csv_export_quotes_untrusted_fields() {
        let report = HealthReportDto::from(HealthReport {
            findings: vec![HealthFinding {
                item_id: "id".to_owned(),
                title: "Compte, \"perso\"".to_owned(),
                primary_url: Some("https://example.test".to_owned()),
                username: None,
                score: 1,
                reused: true,
                compromised: false,
                age_days: Some(400),
            }],
            without_password: 2,
            average_score: Some(1.0),
        });
        assert_eq!(
            report.to_csv(),
            concat!(
                "titre,url,identifiant,score,reutilise,compromis,anciennete_jours\n",
                "\"Compte, \"\"perso\"\"\",https://example.test,,1,oui,non,400\n",
            )
        );
    }
}
