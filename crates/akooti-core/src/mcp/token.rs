//! Scoped MCP tokens.
//!
//! A token is the only authority a headless process (`akooti mcp`) has: it
//! names a set of folders and a set of scopes. The secret is generated once,
//! returned to the UI once, and only its SHA-256 digest is persisted — a dump of
//! the vault cannot be replayed as a token.

use rand_core::{OsRng, RngCore};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use zeroize::Zeroizing;

use crate::error::VaultError;

/// Prefix of every issued token, so leaks are recognisable in logs and diffs.
pub const TOKEN_PREFIX: &str = "ak_mcp_";

/// Random bytes behind one token.
const SECRET_BYTES: usize = 32;

/// Characters of the payload shown in the UI (`ak_mcp_8f9a…`).
const DISPLAY_PREFIX_CHARS: usize = 8;

/// What a token is allowed to do (`docs/schema.md` §1.8).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum McpScope {
    /// Read item metadata (never a secret).
    ReadMetadata,
    /// Ask the daemon to fill a form through the extension.
    BlindFill,
    /// Create an ephemeral account and fill its signup form.
    CreateAndFill,
    /// Inject secrets into a child process environment.
    ExecEnv,
}

impl McpScope {
    /// Stable wire name, also used in the `scopes_json` column.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ReadMetadata => "read_metadata",
            Self::BlindFill => "blind_fill",
            Self::CreateAndFill => "create_and_fill",
            Self::ExecEnv => "exec_env",
        }
    }

    /// Parses a wire name.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "read_metadata" => Some(Self::ReadMetadata),
            "blind_fill" => Some(Self::BlindFill),
            "create_and_fill" => Some(Self::CreateAndFill),
            "exec_env" => Some(Self::ExecEnv),
            _ => None,
        }
    }
}

/// A stored token. Never carries the secret.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpToken {
    /// Identifier.
    pub id: String,
    /// Human label chosen by the user.
    pub name: String,
    /// Display prefix only (`ak_mcp_8f9a…`).
    pub token_prefix: String,
    /// Granted scopes.
    pub scopes: Vec<McpScope>,
    /// Folders this token may reach. Empty means "no folder".
    pub folder_ids: Vec<String>,
    /// Folder used by `vault_create_and_fill`.
    pub default_create_folder_id: Option<String>,
    /// Expiry in Unix seconds.
    pub expires_at: Option<i64>,
    /// Revocation time in Unix seconds.
    pub revoked_at: Option<i64>,
    /// Last successful authentication.
    pub last_used_at: Option<i64>,
    /// Creation time in Unix seconds.
    pub created_at: i64,
}

impl McpToken {
    /// True when the token can no longer authenticate.
    #[must_use]
    pub fn is_active(&self, now: i64) -> bool {
        self.revoked_at.is_none() && self.expires_at.is_none_or(|expiry| expiry > now)
    }
}

/// Token creation payload.
#[derive(Debug, Clone)]
pub struct NewMcpToken {
    /// Human label.
    pub name: String,
    /// Granted scopes.
    pub scopes: Vec<McpScope>,
    /// Allowed folders.
    pub folder_ids: Vec<String>,
    /// Folder used for agent-created items.
    pub default_create_folder_id: Option<String>,
    /// Optional TTL in Unix seconds.
    pub expires_at: Option<i64>,
}

/// A freshly issued token: the secret is shown once and never stored.
#[derive(Debug)]
pub struct IssuedMcpToken {
    /// Stored token metadata.
    pub token: McpToken,
    /// Plaintext secret, already zeroized on drop.
    pub secret: Zeroizing<String>,
}

/// Authenticated caller, without the secret.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpPrincipal {
    /// Token identifier, used for the audit trail.
    pub token_id: String,
    /// Token label, used by `vault_create_and_fill` for `created_by_agent`.
    pub name: String,
    /// Granted scopes.
    pub scopes: Vec<McpScope>,
    /// Allowed folders.
    pub folder_ids: Vec<String>,
    /// Folder used for agent-created items.
    pub default_create_folder_id: Option<String>,
}

impl McpPrincipal {
    /// True when the token carries `scope`.
    #[must_use]
    pub fn has_scope(&self, scope: McpScope) -> bool {
        self.scopes.contains(&scope)
    }

    /// True when `folder_id` is in the token's allowlist.
    #[must_use]
    pub fn allows_folder(&self, folder_id: &str) -> bool {
        self.folder_ids.iter().any(|allowed| allowed == folder_id)
    }

    /// True when the item belongs to at least one allowed folder.
    #[must_use]
    pub fn allows_any_folder(&self, folder_ids: &[String]) -> bool {
        folder_ids
            .iter()
            .any(|folder_id| self.allows_folder(folder_id))
    }

    /// Folder that must receive an item created by this token.
    ///
    /// # Errors
    ///
    /// Returns [`VaultError::Forbidden`] when the token allows no folder.
    pub fn create_folder(&self) -> Result<&str, VaultError> {
        if let Some(folder) = self
            .default_create_folder_id
            .as_deref()
            .filter(|folder| self.allows_folder(folder))
        {
            return Ok(folder);
        }
        self.folder_ids
            .first()
            .map(String::as_str)
            .ok_or(VaultError::Forbidden)
    }
}

/// Generates a new token secret (`ak_mcp_` + 64 hex characters).
///
/// # Errors
///
/// Returns [`VaultError::Crypto`] if the OS random source fails.
pub fn generate_secret() -> Result<Zeroizing<String>, VaultError> {
    let mut bytes = [0u8; SECRET_BYTES];
    OsRng
        .try_fill_bytes(&mut bytes)
        .map_err(|_| VaultError::Crypto)?;
    Ok(Zeroizing::new(format!(
        "{TOKEN_PREFIX}{}",
        hex::encode(bytes)
    )))
}

/// SHA-256 digest of a secret, hex encoded.
#[must_use]
pub fn hash_secret(secret: &str) -> Zeroizing<String> {
    let digest = Sha256::digest(secret.as_bytes());
    Zeroizing::new(hex::encode(digest))
}

/// Display prefix stored for the UI (`ak_mcp_` + 8 characters).
#[must_use]
pub fn display_prefix(secret: &str) -> String {
    let payload = secret.strip_prefix(TOKEN_PREFIX).unwrap_or(secret);
    let visible: String = payload.chars().take(DISPLAY_PREFIX_CHARS).collect();
    format!("{TOKEN_PREFIX}{visible}")
}

/// Compares a stored digest with a presented secret in constant time.
#[must_use]
pub fn secret_matches(stored_hash: &str, secret: &str) -> bool {
    let computed = hash_secret(secret);
    let stored = stored_hash.as_bytes();
    let computed = computed.as_bytes();
    stored.len() == computed.len() && bool::from(stored.ct_eq(computed))
}

#[cfg(test)]
mod tests {
    use super::{
        McpPrincipal, McpScope, TOKEN_PREFIX, display_prefix, generate_secret, hash_secret,
        secret_matches,
    };

    #[test]
    fn secrets_are_prefixed_unique_and_hashed() {
        let first = generate_secret().expect("secret");
        let second = generate_secret().expect("secret");
        assert!(first.starts_with(TOKEN_PREFIX));
        assert_eq!(first.len(), TOKEN_PREFIX.len() + 64);
        assert_ne!(*first, *second);
        assert_ne!(*hash_secret(&first), *hash_secret(&second));
        assert_eq!(hash_secret(&first).len(), 64);
    }

    #[test]
    fn a_secret_only_matches_its_own_digest() {
        let secret = generate_secret().expect("secret");
        let digest = hash_secret(&secret);
        assert!(secret_matches(&digest, &secret));
        assert!(!secret_matches(&digest, "ak_mcp_deadbeef"));
        assert!(!secret_matches("short", &secret));
    }

    #[test]
    fn display_prefix_never_exceeds_eight_characters() {
        let prefix = display_prefix("ak_mcp_0123456789abcdef");
        assert_eq!(prefix, "ak_mcp_01234567");
        assert_eq!(display_prefix("ak_mcp_ab"), "ak_mcp_ab");
        assert_eq!(display_prefix("naked"), "ak_mcp_naked");
    }

    #[test]
    fn scopes_round_trip_through_their_wire_names() {
        for scope in [
            McpScope::ReadMetadata,
            McpScope::BlindFill,
            McpScope::CreateAndFill,
            McpScope::ExecEnv,
        ] {
            assert_eq!(McpScope::parse(scope.as_str()), Some(scope));
        }
        assert_eq!(McpScope::parse("root"), None);
    }

    #[test]
    fn principal_can_only_target_allowed_folders() {
        let principal = McpPrincipal {
            token_id: "t".to_owned(),
            name: "agent".to_owned(),
            scopes: vec![McpScope::ReadMetadata],
            folder_ids: vec!["dev".to_owned()],
            default_create_folder_id: None,
        };
        assert!(principal.allows_folder("dev"));
        assert!(!principal.allows_folder("personal"));
        assert!(principal.allows_any_folder(&["personal".to_owned(), "dev".to_owned()]));
        assert!(!principal.allows_any_folder(&["personal".to_owned()]));
        assert_eq!(principal.create_folder().expect("folder"), "dev");
        assert!(!principal.has_scope(McpScope::ExecEnv));

        let empty = McpPrincipal {
            folder_ids: Vec::new(),
            ..principal
        };
        assert!(empty.create_folder().is_err());
    }
}
