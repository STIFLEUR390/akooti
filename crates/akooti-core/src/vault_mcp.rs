use std::time::SystemTime;

use regex::Regex;
use url::Url;

use crate::error::VaultError;
use crate::mcp::audit::{self, AuditStatus, McpAction, McpAuditEntry, NewAuditEntry};
use crate::mcp::store;
use crate::mcp::token::{
    IssuedMcpToken, McpPrincipal, McpScope, McpToken, NewMcpToken, display_prefix, generate_secret,
    hash_secret, secret_matches,
};
use crate::models::{
    ItemFilter, ItemType, ItemUri, SecretField, SecretString, Selectors, UriMatchType, VaultItem,
    VaultItemDraft,
};
use crate::vault::{Vault, unix_seconds};

/// Values the daemon may inject for one item. Never serialised to an agent.
#[derive(Debug)]
pub struct McpFillPlan {
    /// Selectors captured for blind filling.
    pub selectors: Selectors,
    /// Username or account identifier.
    pub username: Option<SecretString>,
    /// Password, when stored.
    pub password: Option<SecretString>,
    /// Current TOTP code (not the seed).
    pub totp: Option<SecretString>,
}

/// Values released to the paired browser extension for one origin.
///
/// Debug prints the origin and the presence of each value, never a value.
#[derive(Debug)]
pub struct ExtensionFill {
    /// Origin the values are released for.
    pub origin: String,
    /// Selectors captured on the item.
    pub selectors: Selectors,
    /// Account identifier, when the item has one.
    pub username: Option<SecretString>,
    /// Password, when the item has one.
    pub password: Option<SecretString>,
    /// Current TOTP code (never the seed), when the item has one.
    pub totp: Option<SecretString>,
}

/// Ephemeral account requested by an agent.
#[derive(Debug, Clone)]
pub struct EphemeralAccountDraft {
    /// Title shown in the vault.
    pub title: String,
    /// Origin the account belongs to.
    pub primary_url: Option<String>,
    /// Username to register.
    pub username: Option<String>,
    /// Lifetime in seconds, after which the vault purges the item.
    pub ttl_seconds: i64,
    /// Requested folder; must be allowed for the token.
    pub folder_id: Option<String>,
}

/// Result of an ephemeral creation.
#[derive(Debug)]
pub struct EphemeralAccount {
    /// Stored item metadata.
    pub item: VaultItem,
    /// Injection plan for the signup form.
    pub plan: McpFillPlan,
}

impl Vault {
    /// Issues a new token. The secret is returned exactly once.
    ///
    /// # Errors
    ///
    /// Returns [`VaultError::InvalidInput`] for an empty name or scope list, an
    /// unknown folder, or a default folder outside the allowlist.
    pub fn create_mcp_token(&mut self, draft: NewMcpToken) -> Result<IssuedMcpToken, VaultError> {
        let name = draft.name.trim();
        if name.is_empty() || name.chars().count() > 100 {
            return Err(VaultError::InvalidInput("token name is invalid"));
        }
        if draft.scopes.is_empty() {
            return Err(VaultError::InvalidInput("a token needs at least one scope"));
        }
        if draft.folder_ids.is_empty() {
            return Err(VaultError::InvalidInput(
                "a token needs at least one folder",
            ));
        }
        let mut unique = Vec::new();
        for folder_id in &draft.folder_ids {
            crate::folders::get(self.connection(), folder_id)?;
            if !unique.contains(folder_id) {
                unique.push(folder_id.clone());
            }
        }
        if let Some(default) = draft.default_create_folder_id.as_deref()
            && !unique.iter().any(|folder| folder == default)
        {
            return Err(VaultError::InvalidInput(
                "the default create folder must be allowed",
            ));
        }

        let secret = generate_secret()?;
        let id = uuid::Uuid::new_v4().to_string();
        let now = unix_seconds(SystemTime::now())?;
        if draft.expires_at.is_some_and(|expiry| expiry <= now) {
            return Err(VaultError::InvalidInput("token expiry is in the past"));
        }
        store::insert(
            self.connection(),
            &id,
            name,
            &hash_secret(&secret),
            &display_prefix(&secret),
            &draft.scopes,
            &unique,
            draft.default_create_folder_id.as_deref(),
            draft.expires_at,
            now,
        )?;
        let token = self
            .mcp_tokens()?
            .into_iter()
            .find(|token| token.id == id)
            .ok_or(VaultError::CorruptVault)?;
        Ok(IssuedMcpToken { token, secret })
    }

    /// Lists every token, revoked or not. Never returns a secret.
    ///
    /// # Errors
    ///
    /// Returns a database error when the token table cannot be read.
    pub fn mcp_tokens(&self) -> Result<Vec<McpToken>, VaultError> {
        store::list(self.connection())
    }

    /// Revokes one token immediately.
    ///
    /// # Errors
    ///
    /// Returns [`VaultError::NotFound`] for an unknown or already revoked token.
    pub fn revoke_mcp_token(&mut self, id: &str) -> Result<McpToken, VaultError> {
        store::set_revoked(self.connection(), id, unix_seconds(SystemTime::now())?)
    }

    /// Deletes one token and its folder allowlist.
    ///
    /// # Errors
    ///
    /// Returns [`VaultError::NotFound`] for an unknown token.
    pub fn delete_mcp_token(&mut self, id: &str) -> Result<(), VaultError> {
        store::delete(self.connection(), id)
    }

    /// Authenticates a presented secret and refreshes `last_used_at`.
    ///
    /// # Errors
    ///
    /// Returns [`VaultError::InvalidCredentials`] for an unknown, revoked or
    /// expired token. The error variant is identical in all three cases so the
    /// caller cannot probe for token existence.
    pub fn authenticate_mcp_token(&mut self, secret: &str) -> Result<McpPrincipal, VaultError> {
        let digest = hash_secret(secret);
        let Some(stored) = store::by_hash(self.connection(), &digest)? else {
            return Err(VaultError::InvalidCredentials);
        };
        if !secret_matches(&stored.token_hash, secret) {
            return Err(VaultError::InvalidCredentials);
        }
        let now = unix_seconds(SystemTime::now())?;
        if !stored.token.is_active(now) {
            return Err(VaultError::InvalidCredentials);
        }
        store::touch(self.connection(), &stored.token.id, now)?;
        Ok(McpPrincipal {
            token_id: stored.token.id,
            name: stored.token.name,
            scopes: stored.token.scopes,
            folder_ids: stored.token.folder_ids,
            default_create_folder_id: stored.token.default_create_folder_id,
        })
    }

    /// Metadata-only search, restricted to the token's folders.
    ///
    /// # Errors
    ///
    /// Returns [`VaultError::Forbidden`] without the `read_metadata` scope.
    pub fn mcp_search(
        &self,
        principal: &McpPrincipal,
        query: &str,
        limit: usize,
    ) -> Result<Vec<VaultItem>, VaultError> {
        if !principal.has_scope(McpScope::ReadMetadata) {
            return Err(VaultError::Forbidden);
        }
        let needle = query.trim().to_lowercase();
        let items = self.list_items(&ItemFilter::default())?;
        let mut matches: Vec<VaultItem> = items
            .into_iter()
            .filter(|item| principal.allows_any_folder(&item.folder_ids))
            .filter(|item| {
                needle.is_empty()
                    || item.title.to_lowercase().contains(&needle)
                    || item
                        .username
                        .as_deref()
                        .is_some_and(|value| value.to_lowercase().contains(&needle))
                    || item
                        .primary_url
                        .as_deref()
                        .is_some_and(|value| value.to_lowercase().contains(&needle))
            })
            .collect();
        matches.truncate(limit);
        record(
            self,
            principal,
            McpAction::VaultSearch,
            AuditStatus::Success,
            None,
            None,
            None,
            Some(format!("{} result(s)", matches.len())),
        )?;
        Ok(matches)
    }

    /// Authorises an action on one item without decrypting anything.
    ///
    /// Used by transports that are not wired yet (the extension bridge), so a
    /// refusal is still reported and a secret is never unwrapped for nothing.
    /// Writing the audit row is the daemon's job, since it knows the action.
    ///
    /// # Errors
    ///
    /// Returns [`VaultError::Forbidden`] when the scope or folder is missing,
    /// [`VaultError::NotFound`] for an unknown item.
    pub fn mcp_authorize(
        &self,
        principal: &McpPrincipal,
        item_id: &str,
        scope: McpScope,
    ) -> Result<VaultItem, VaultError> {
        if !principal.has_scope(scope) {
            return Err(VaultError::Forbidden);
        }
        let item = self.get_item(item_id)?;
        if !principal.allows_any_folder(&item.folder_ids) {
            return Err(VaultError::Forbidden);
        }
        Ok(item)
    }

    /// Resolves the values needed to fill one form.
    ///
    /// This is the only helper that decrypts a secret for an agent-initiated
    /// action, and its output never leaves the daemon process.
    ///
    /// # Errors
    ///
    /// Returns [`VaultError::Forbidden`] when the scope or folder is missing,
    /// [`VaultError::NotFound`] for an unknown item.
    pub fn mcp_fill_plan(
        &self,
        principal: &McpPrincipal,
        item_id: &str,
        scope: McpScope,
    ) -> Result<McpFillPlan, VaultError> {
        if !principal.has_scope(scope) {
            return Err(VaultError::Forbidden);
        }
        let item = self.get_item(item_id)?;
        if !principal.allows_any_folder(&item.folder_ids) {
            return Err(VaultError::Forbidden);
        }
        let username = item.username.clone().map(SecretString::new);
        let password = if item.has_password {
            self.reveal_secret(item_id, SecretField::Password, None)
                .ok()
                .map(|secret| secret.value)
        } else {
            None
        };
        let totp = if item.has_totp {
            let timestamp = u64::try_from(unix_seconds(SystemTime::now())?).unwrap_or(0);
            self.generate_totp(item_id, crate::TotpAlgorithm::Sha1, timestamp)
                .ok()
        } else {
            None
        };
        Ok(McpFillPlan {
            selectors: item.selectors.clone(),
            username,
            password,
            totp,
        })
    }

    /// Releases the values of one item to the paired browser extension.
    ///
    /// This is the only path that hands a secret to another process; it exists
    /// for the extension, which fills a page or copies a value after an
    /// explicit user gesture (`docs/architecture.md` §4.5). The token still
    /// needs `blind_fill`, the folder allowlist still applies, and the origin
    /// must be one the item claims: the extension cannot ask for a value on a
    /// page the item was not saved for.
    ///
    /// # Errors
    ///
    /// Returns [`VaultError::Forbidden`] when the scope, the folder or the
    /// origin does not match, and [`VaultError::NotFound`] for an unknown item.
    pub fn mcp_extension_fill(
        &self,
        principal: &McpPrincipal,
        item_id: &str,
        origin: &str,
        scope: McpScope,
    ) -> Result<ExtensionFill, VaultError> {
        let item = self.mcp_authorize(principal, item_id, scope)?;
        if !origin_matches(origin, item.primary_url.as_deref(), &item.uris) {
            return Err(VaultError::Forbidden);
        }
        let username = item.username.clone().map(SecretString::new);
        let password = if item.has_password {
            self.reveal_secret(item_id, SecretField::Password, None)
                .ok()
                .map(|secret| secret.value)
        } else {
            None
        };
        let totp = if item.has_totp {
            let timestamp = u64::try_from(unix_seconds(SystemTime::now())?).unwrap_or(0);
            self.generate_totp(item_id, crate::TotpAlgorithm::Sha1, timestamp)
                .ok()
        } else {
            None
        };
        Ok(ExtensionFill {
            origin: origin.to_owned(),
            selectors: item.selectors.clone(),
            username,
            password,
            totp,
        })
    }

    /// Creates an ephemeral account in the token's folder and returns its plan.    ///
    /// # Errors
    ///
    /// Returns [`VaultError::Forbidden`] without `create_and_fill` or when the
    /// requested folder is not allowed.
    pub fn mcp_create_ephemeral(
        &mut self,
        principal: &McpPrincipal,
        draft: EphemeralAccountDraft,
    ) -> Result<EphemeralAccount, VaultError> {
        if !principal.has_scope(McpScope::CreateAndFill) {
            return Err(VaultError::Forbidden);
        }
        let folder_id = match draft.folder_id.as_deref() {
            Some(requested) => {
                if !principal.allows_folder(requested) {
                    return Err(VaultError::Forbidden);
                }
                requested.to_owned()
            }
            None => principal.create_folder()?.to_owned(),
        };
        let ttl = draft.ttl_seconds.clamp(60, 60 * 60 * 24 * 30);
        let now = unix_seconds(SystemTime::now())?;
        let generated = crate::generate_password(&crate::PasswordOptions {
            length: 24,
            lowercase: true,
            uppercase: true,
            digits: true,
            symbols: true,
            exclude_ambiguous: true,
        })?;
        let password = SecretString::new(generated.value.expose().to_owned());
        let item = self.create_item(VaultItemDraft {
            item_type: ItemType::Login,
            title: draft.title,
            primary_url: draft.primary_url.clone(),
            username: draft.username,
            password: Some(password),
            totp_secret: None,
            notes: Some(SecretString::new(format!(
                "Compte éphémère créé par le jeton MCP « {} ».",
                principal.name
            ))),
            selectors: Selectors::default(),
            created_by_agent: Some(principal.name.clone()),
            expires_at: Some(now + ttl),
            favorite: false,
            folder_ids: vec![folder_id.clone()],
            uris: Vec::new(),
            custom_fields: Vec::new(),
        })?;
        // The daemon writes the audit row once the fill outcome is known, so a
        // refusal to fill still shows up as such.
        let plan = self.mcp_fill_plan(principal, &item.id, McpScope::CreateAndFill)?;
        Ok(EphemeralAccount { item, plan })
    }

    /// Environment variables exported by an item for `vault_exec`.
    ///
    /// # Errors
    ///
    /// Returns [`VaultError::Forbidden`] without `exec_env` or outside the
    /// token's folders.
    pub fn mcp_env(
        &self,
        principal: &McpPrincipal,
        item_id: &str,
    ) -> Result<Vec<(String, SecretString)>, VaultError> {
        if !principal.has_scope(McpScope::ExecEnv) {
            return Err(VaultError::Forbidden);
        }
        let item = self.get_item(item_id)?;
        if !principal.allows_any_folder(&item.folder_ids) {
            return Err(VaultError::Forbidden);
        }
        let mut variables = Vec::new();
        if let Some(username) = item.username.as_deref() {
            variables.push((
                "AKOOTI_USERNAME".to_owned(),
                SecretString::new(username.to_owned()),
            ));
        }
        if item.has_password
            && let Ok(secret) = self.reveal_secret(item_id, SecretField::Password, None)
        {
            variables.push(("AKOOTI_PASSWORD".to_owned(), secret.value));
        }
        for field in &item.custom_fields {
            if let Ok(secret) =
                self.reveal_secret(item_id, SecretField::CustomField, Some(&field.id))
            {
                variables.push((env_name(&field.field_name), secret.value));
            }
        }
        Ok(variables)
    }

    /// Appends one audit row.
    ///
    /// # Errors
    ///
    /// Returns a database error when the log cannot be written.
    pub fn record_audit(&self, entry: NewAuditEntry) -> Result<(), VaultError> {
        audit::insert(self.connection(), &entry, unix_seconds(SystemTime::now())?)
    }

    /// Reads the audit trail, newest first.
    ///
    /// # Errors
    ///
    /// Returns a database error when the log cannot be read.
    pub fn mcp_audit_logs(&self, limit: usize) -> Result<Vec<McpAuditEntry>, VaultError> {
        audit::list(self.connection(), limit)
    }
}

fn env_name(field: &str) -> String {
    let upper = field
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .collect::<String>();
    format!("AKOOTI_{upper}")
}

#[allow(clippy::too_many_arguments)]
fn record(
    vault: &Vault,
    principal: &McpPrincipal,
    action: McpAction,
    status: AuditStatus,
    item_id: Option<&str>,
    folder_id: Option<&str>,
    origin: Option<&str>,
    details: Option<String>,
) -> Result<(), VaultError> {
    vault.record_audit(NewAuditEntry {
        token_id: Some(principal.token_id.clone()),
        action,
        target_item_id: item_id.map(str::to_owned),
        target_folder_id: folder_id.map(str::to_owned),
        target_origin: origin.map(str::to_owned),
        status,
        details,
    })
}

/// Whether `origin` belongs to the item.
///
/// Only `http(s)` origins qualify. The primary URL is compared on its origin
/// (scheme, host, port — never the path); each extra URI is matched with its
/// own strategy, which is what makes an item found on `app.example.test` also
/// usable on `example.test` when it was saved with a `domain` match.
fn origin_matches(origin: &str, primary_url: Option<&str>, uris: &[ItemUri]) -> bool {
    let Ok(requested) = Url::parse(origin) else {
        return false;
    };
    if !matches!(requested.scheme(), "http" | "https") {
        return false;
    }
    let requested_host = requested
        .host_str()
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    if requested_host.is_empty() {
        return false;
    }
    let requested_origin = requested.origin();
    if primary_url
        .and_then(|value| Url::parse(value).ok())
        .is_some_and(|primary| primary.origin() == requested_origin)
    {
        return true;
    }
    uris.iter().any(|entry| match entry.match_type {
        UriMatchType::Domain => Url::parse(&entry.uri)
            .ok()
            .and_then(|parsed| parsed.host_str().map(str::to_ascii_lowercase))
            .is_some_and(|host| {
                requested_host == host || requested_host.ends_with(&format!(".{host}"))
            }),
        UriMatchType::Host => Url::parse(&entry.uri)
            .ok()
            .and_then(|parsed| parsed.host_str().map(str::to_owned))
            .is_some_and(|host| host.eq_ignore_ascii_case(&requested_host)),
        UriMatchType::StartsWith => origin.starts_with(entry.uri.trim_end_matches('/')),
        UriMatchType::Exact => Url::parse(&entry.uri).is_ok_and(|exact| {
            exact.origin() == requested_origin && exact.path() == requested.path()
        }),
        UriMatchType::Regex => Regex::new(&entry.uri).is_ok_and(|pattern| pattern.is_match(origin)),
    })
}

#[cfg(test)]
mod tests {
    use super::origin_matches;
    use crate::models::{ItemUri, UriMatchType};

    fn uri(value: &str, match_type: UriMatchType) -> ItemUri {
        ItemUri {
            id: "uri-1".to_owned(),
            uri: value.to_owned(),
            match_type,
        }
    }

    #[test]
    fn the_primary_url_matches_on_its_origin_only() {
        assert!(origin_matches(
            "https://app.example.test",
            Some("https://app.example.test/login?next=/"),
            &[]
        ));
        assert!(!origin_matches(
            "https://app.example.test",
            Some("https://example.test"),
            &[]
        ));
        assert!(!origin_matches(
            "http://app.example.test",
            Some("https://app.example.test"),
            &[]
        ));
    }

    #[test]
    fn a_domain_uri_covers_subdomains_but_not_lookalikes() {
        let uris = [uri("https://example.test/login", UriMatchType::Domain)];
        assert!(origin_matches("https://example.test", None, &uris));
        assert!(origin_matches("https://app.example.test", None, &uris));
        assert!(!origin_matches("https://evil-example.test", None, &uris));
        assert!(!origin_matches("https://example.test.evil", None, &uris));
    }

    #[test]
    fn host_and_starts_with_and_exact_follow_their_strategy() {
        let host = [uri("https://example.test", UriMatchType::Host)];
        assert!(origin_matches("https://example.test", None, &host));
        assert!(!origin_matches("https://app.example.test", None, &host));

        let prefix = [uri("https://example.test/app", UriMatchType::StartsWith)];
        assert!(origin_matches("https://example.test/app", None, &prefix));
        assert!(!origin_matches("https://example.test", None, &prefix));

        let exact = [uri("https://example.test/app", UriMatchType::Exact)];
        assert!(origin_matches("https://example.test/app", None, &exact));
        assert!(!origin_matches("https://example.test/other", None, &exact));

        let regex = [uri(
            r"^https://(app|www)\.example\.test$",
            UriMatchType::Regex,
        )];
        assert!(origin_matches("https://www.example.test", None, &regex));
        assert!(!origin_matches("https://evil.example.test", None, &regex));
        // A malformed pattern never matches instead of failing the request.
        let broken = [uri("https://(", UriMatchType::Regex)];
        assert!(!origin_matches("https://example.test", None, &broken));
    }

    #[test]
    fn only_http_origins_qualify() {
        assert!(!origin_matches("file:///etc/passwd", None, &[]));
        assert!(!origin_matches("chrome-extension://abc", None, &[]));
        assert!(!origin_matches("not a url", None, &[]));
        assert!(!origin_matches("https://app.example.test/admin", None, &[]));
    }

    #[test]
    fn a_subdomain_uri_cannot_claim_its_parent_origin() {
        let uris = [uri("https://app.example.test", UriMatchType::Domain)];
        assert!(origin_matches("https://app.example.test", None, &uris));
        assert!(!origin_matches("https://example.test", None, &uris));
    }
}
