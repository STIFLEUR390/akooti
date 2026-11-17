use std::fmt;

use zeroize::Zeroizing;

/// Stable identifier for the Personal system folder.
pub const PERSONAL_FOLDER_ID: &str = "00000000-0000-4000-8000-000000000001";
/// Stable identifier for the Dev Local system folder.
pub const DEV_LOCAL_FOLDER_ID: &str = "00000000-0000-4000-8000-000000000002";
/// Stable identifier for the Agents & MCP system folder.
pub const AGENTS_MCP_FOLDER_ID: &str = "00000000-0000-4000-8000-000000000003";

/// Supported vault item kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemType {
    /// Login credentials.
    Login,
    /// API credentials.
    ApiKey,
    /// Secure note.
    Note,
    /// Environment variable set.
    EnvSet,
}

impl ItemType {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Login => "login",
            Self::ApiKey => "api_key",
            Self::Note => "note",
            Self::EnvSet => "env_set",
        }
    }

    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "login" => Some(Self::Login),
            "api_key" => Some(Self::ApiKey),
            "note" => Some(Self::Note),
            "env_set" => Some(Self::EnvSet),
            _ => None,
        }
    }
}

/// URI matching strategy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UriMatchType {
    /// Match by registrable domain.
    Domain,
    /// Match the exact host.
    Host,
    /// Match a URL prefix.
    StartsWith,
    /// Match the complete URL.
    Exact,
    /// Match a regular expression.
    Regex,
}

impl UriMatchType {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Domain => "domain",
            Self::Host => "host",
            Self::StartsWith => "starts_with",
            Self::Exact => "exact",
            Self::Regex => "regex",
        }
    }

    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "domain" => Some(Self::Domain),
            "host" => Some(Self::Host),
            "starts_with" => Some(Self::StartsWith),
            "exact" => Some(Self::Exact),
            "regex" => Some(Self::Regex),
            _ => None,
        }
    }
}

/// A secret value that is zeroed when dropped.
pub struct SecretString(Zeroizing<String>);

impl SecretString {
    /// Wraps an owned secret.
    pub fn new(value: String) -> Self {
        Self(Zeroizing::new(value))
    }

    /// Borrows the secret without cloning it.
    pub fn expose(&self) -> &str {
        self.0.as_str()
    }
}

impl fmt::Debug for SecretString {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SecretString([REDACTED])")
    }
}

/// CSS selectors saved for blind autofill.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Selectors {
    /// Username field selector.
    pub username: Option<String>,
    /// Password field selector.
    pub password: Option<String>,
    /// TOTP field selector.
    pub totp: Option<String>,
    /// Submit control selector.
    pub submit: Option<String>,
}

/// Folder metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Folder {
    /// Folder identifier.
    pub id: String,
    /// Optional parent folder.
    pub parent_id: Option<String>,
    /// Display name.
    pub name: String,
    /// Stable system slug.
    pub slug: Option<String>,
    /// Icon name.
    pub icon: String,
    /// Optional display colour.
    pub color: Option<String>,
    /// Whether this is a protected system folder.
    pub is_system: bool,
    /// Whether MCP access requires biometric confirmation.
    pub require_biometric_for_mcp: bool,
    /// Creation time in Unix seconds.
    pub created_at: i64,
    /// Last update time in Unix seconds.
    pub updated_at: i64,
    /// Soft-delete time in Unix seconds.
    pub deleted_at: Option<i64>,
}

/// Folder creation or full update input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderDraft {
    /// Optional parent folder.
    pub parent_id: Option<String>,
    /// Display name.
    pub name: String,
    /// Icon name.
    pub icon: String,
    /// Optional display colour.
    pub color: Option<String>,
    /// Whether MCP access requires biometric confirmation.
    pub require_biometric_for_mcp: bool,
}

/// URI linked to an item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemUri {
    /// URI identifier.
    pub id: String,
    /// URL or pattern.
    pub uri: String,
    /// Matching strategy.
    pub match_type: UriMatchType,
}

/// URI replacement input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemUriInput {
    /// URL or pattern.
    pub uri: String,
    /// Matching strategy.
    pub match_type: UriMatchType,
}

/// Custom field metadata without its secret value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomField {
    /// Field identifier.
    pub id: String,
    /// Field name.
    pub field_name: String,
    /// Whether the value should be masked in the UI.
    pub is_secret: bool,
    /// Display order.
    pub sort_order: i64,
}

/// Custom field replacement input.
pub struct CustomFieldInput {
    /// Field name.
    pub field_name: String,
    /// Value, encrypted before storage.
    pub value: SecretString,
    /// Whether the value should be masked in the UI.
    pub is_secret: bool,
    /// Display order.
    pub sort_order: i64,
}

impl fmt::Debug for CustomFieldInput {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CustomFieldInput")
            .field("field_name", &self.field_name)
            .field("value", &"[REDACTED]")
            .field("is_secret", &self.is_secret)
            .field("sort_order", &self.sort_order)
            .finish()
    }
}

/// Vault item metadata, excluding plaintext secret values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VaultItem {
    /// Item identifier.
    pub id: String,
    /// Item kind.
    pub item_type: ItemType,
    /// Display title.
    pub title: String,
    /// Primary URL.
    pub primary_url: Option<String>,
    /// Username or account identifier.
    pub username: Option<String>,
    /// Saved autofill selectors.
    pub selectors: Selectors,
    /// zxcvbn score, when calculated.
    pub zxcvbn_score: Option<i64>,
    /// SHA-1 prefix used for k-anonymity checks.
    pub password_sha1_prefix: Option<String>,
    /// Whether the password is known compromised.
    pub is_compromised: bool,
    /// Password change time in Unix seconds.
    pub password_updated_at: Option<i64>,
    /// Agent that created an ephemeral item.
    pub created_by_agent: Option<String>,
    /// Expiry time in Unix seconds.
    pub expires_at: Option<i64>,
    /// Whether the item is a favourite.
    pub favorite: bool,
    /// Folder memberships.
    pub folder_ids: Vec<String>,
    /// Linked URIs.
    pub uris: Vec<ItemUri>,
    /// Linked custom fields.
    pub custom_fields: Vec<CustomField>,
    /// Creation time in Unix seconds.
    pub created_at: i64,
    /// Last update time in Unix seconds.
    pub updated_at: i64,
    /// Soft-delete time in Unix seconds.
    pub deleted_at: Option<i64>,
    /// Whether an encrypted password is stored (presence only, never the value).
    pub has_password: bool,
    /// Whether an encrypted TOTP seed is stored.
    pub has_totp: bool,
    /// Whether encrypted notes are stored.
    pub has_notes: bool,
}

/// Vault item creation or full update input.
pub struct VaultItemDraft {
    /// Item kind.
    pub item_type: ItemType,
    /// Display title.
    pub title: String,
    /// Primary URL.
    pub primary_url: Option<String>,
    /// Username or account identifier.
    pub username: Option<String>,
    /// Password secret.
    pub password: Option<SecretString>,
    /// TOTP secret.
    pub totp_secret: Option<SecretString>,
    /// Secure notes.
    pub notes: Option<SecretString>,
    /// Saved autofill selectors.
    pub selectors: Selectors,
    /// Agent that created an ephemeral item.
    pub created_by_agent: Option<String>,
    /// Expiry time in Unix seconds.
    pub expires_at: Option<i64>,
    /// Whether the item is a favourite.
    pub favorite: bool,
    /// Folder memberships.
    pub folder_ids: Vec<String>,
    /// Linked URIs.
    pub uris: Vec<ItemUriInput>,
    /// Linked custom fields.
    pub custom_fields: Vec<CustomFieldInput>,
}

impl fmt::Debug for VaultItemDraft {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("VaultItemDraft")
            .field("item_type", &self.item_type)
            .field("title", &self.title)
            .field("primary_url", &self.primary_url)
            .field("username", &self.username)
            .field("password", &self.password)
            .field("totp_secret", &self.totp_secret)
            .field("notes", &self.notes)
            .field("selectors", &self.selectors)
            .field("created_by_agent", &self.created_by_agent)
            .field("expires_at", &self.expires_at)
            .field("favorite", &self.favorite)
            .field("folder_ids", &self.folder_ids)
            .field("uris", &self.uris)
            .field("custom_fields", &self.custom_fields)
            .finish()
    }
}

/// Item listing filter.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ItemFilter {
    /// Include soft-deleted records.
    pub include_deleted: bool,
    /// Restrict results to one folder.
    pub folder_id: Option<String>,
}

/// Secret field available to an explicit desktop reveal operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretField {
    /// Item password.
    Password,
    /// Item TOTP seed.
    Totp,
    /// Item notes.
    Notes,
    /// Custom field selected by identifier.
    CustomField,
}

impl SecretField {
    /// Stable wire name used by the desktop UI and the IPC protocols.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Password => "password",
            Self::Totp => "totp",
            Self::Notes => "notes",
            Self::CustomField => "custom_field",
        }
    }
}

/// Metadata-only item edit that never needs a plaintext secret.
#[derive(Clone)]
pub struct ItemMetadataDraft {
    /// Item kind.
    pub item_type: ItemType,
    /// Display title.
    pub title: String,
    /// Primary URL.
    pub primary_url: Option<String>,
    /// Username or account identifier.
    pub username: Option<String>,
    /// Saved autofill selectors.
    pub selectors: Selectors,
    /// Expiry time in Unix seconds.
    pub expires_at: Option<i64>,
    /// Whether the item is a favourite.
    pub favorite: bool,
    /// Folder memberships after the edit.
    pub folder_ids: Vec<String>,
    /// Linked URIs after the edit.
    pub uris: Vec<ItemUriInput>,
}

impl fmt::Debug for ItemMetadataDraft {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ItemMetadataDraft")
            .field("item_type", &self.item_type)
            .field("title", &self.title)
            .field("primary_url", &self.primary_url)
            .field("username", &self.username)
            .field("selectors", &self.selectors)
            .field("expires_at", &self.expires_at)
            .field("favorite", &self.favorite)
            .field("folder_ids", &self.folder_ids)
            .field("uris", &self.uris)
            .finish()
    }
}

/// A secret revealed explicitly to the local UI.
pub struct RevealedSecret {
    /// Item identifier.
    pub item_id: String,
    /// Selected field.
    pub field: SecretField,
    /// Plaintext value, zeroed after use.
    pub value: SecretString,
}

impl fmt::Debug for RevealedSecret {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RevealedSecret")
            .field("item_id", &self.item_id)
            .field("field", &self.field)
            .field("value", &"[REDACTED]")
            .finish()
    }
}

/// Current in-memory lock state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VaultStatus {
    /// No database connection or master key is held.
    Locked,
    /// The vault is ready for sensitive operations.
    Unlocked,
}
