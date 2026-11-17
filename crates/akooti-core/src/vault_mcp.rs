use std::time::SystemTime;

use crate::error::VaultError;
use crate::mcp::audit::{self, AuditStatus, McpAction, McpAuditEntry, NewAuditEntry};
use crate::mcp::store;
use crate::mcp::token::{
    IssuedMcpToken, McpPrincipal, McpScope, McpToken, NewMcpToken, display_prefix, generate_secret,
    hash_secret, secret_matches,
};
use crate::models::{
    ItemFilter, ItemType, SecretField, SecretString, Selectors, VaultItem, VaultItemDraft,
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

    /// Creates an ephemeral account in the token's folder and returns its plan.
    ///
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
