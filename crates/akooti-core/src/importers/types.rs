use crate::models::VaultItemDraft;

/// Supported import source formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ImportFormat {
    /// Native encrypted Buttercup vault.
    Buttercup,
    /// Buttercup CSV export.
    ButtercupCsv,
    /// Bitwarden or Vaultwarden JSON export.
    BitwardenJson,
    /// Proton Pass JSON or unencrypted ZIP export.
    ProtonPass,
    /// KeePass KDB, KDBX 3, or KDBX 4 database.
    KeePass,
    /// KeePass 2 XML export.
    KeePassXml,
    /// KeePass or KeePassXC CSV export.
    KeePassCsv,
    /// 1Password `.1pux` archive.
    OnePasswordPux,
    /// 1Password CSV export.
    OnePasswordCsv,
    /// Chrome, Brave, Edge, or Firefox CSV export.
    BrowserCsv,
}

/// Folder captured in a normalized import bundle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportFolder {
    /// Identifier in the source export.
    pub external_id: String,
    /// Display name.
    pub name: String,
    /// Parent identifier in the source export.
    pub parent_external_id: Option<String>,
}

/// Item captured in a normalized import bundle.
pub struct ImportItem {
    /// Identifier in the source export.
    pub external_id: String,
    /// Draft whose folder identifiers still reference the source export.
    pub draft: VaultItemDraft,
}

impl std::fmt::Debug for ImportItem {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ImportItem")
            .field("external_id", &self.external_id)
            .field("draft", &self.draft)
            .finish()
    }
}

/// Non-fatal parser or import warning.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportIssue {
    /// One-based source line when available.
    pub line: Option<usize>,
    /// Stable issue category.
    pub kind: ImportIssueKind,
}

/// Categories of non-fatal import issues.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ImportIssueKind {
    /// A row did not contain enough information.
    InvalidRow,
    /// An item had no usable title.
    MissingTitle,
    /// An item type is not represented by Akooti.
    UnsupportedItem,
    /// A protected XML value could not be read.
    ProtectedXmlValue,
    /// A supplied TOTP value was malformed.
    InvalidTotp,
    /// A source folder graph could not be resolved.
    InvalidFolder,
    /// The source file could not be securely removed after commit.
    SecureDeleteFailed,
}

/// Normalized data produced by every source parser.
pub struct ImportBundle {
    /// Detected or selected source format.
    pub format: ImportFormat,
    /// Source folders.
    pub folders: Vec<ImportFolder>,
    /// Source items with unmodified folder references.
    pub items: Vec<ImportItem>,
    /// Recoverable parser warnings.
    pub issues: Vec<ImportIssue>,
}

impl std::fmt::Debug for ImportBundle {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ImportBundle")
            .field("format", &self.format)
            .field("folders", &self.folders)
            .field("items", &self.items)
            .field("issues", &self.issues)
            .finish()
    }
}

/// User-controlled import behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImportOptions {
    /// Add matching local entries to `Dev Local` without removing other folders.
    pub route_dev_local: bool,
    /// Behavior for entries marked as duplicates by preview.
    pub duplicate_policy: DuplicatePolicy,
    /// Overwrite and remove the source after a committed import.
    pub secure_delete_source: bool,
}

impl Default for ImportOptions {
    fn default() -> Self {
        Self {
            route_dev_local: true,
            duplicate_policy: DuplicatePolicy::Skip,
            secure_delete_source: false,
        }
    }
}

/// Choice applied to entries detected as duplicates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DuplicatePolicy {
    /// Keep the existing item and skip the imported copy.
    Skip,
    /// Import the source item even when it resembles an existing entry.
    ImportAnyway,
}

/// Reason an imported item resembles an existing item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DuplicateReason {
    /// Normalized URL and username are equal.
    UrlAndUsername,
    /// Titles are highly similar.
    FuzzyTitle,
}

/// One likely duplicate found before committing.
#[derive(Debug, Clone, PartialEq)]
pub struct DuplicateMatch {
    /// Identifier from the source bundle.
    pub imported_external_id: String,
    /// Existing Akooti item identifier.
    pub existing_item_id: String,
    /// Why the pair matched.
    pub reason: DuplicateReason,
    /// Similarity score from 0.0 to 1.0.
    pub score: f64,
}

/// Read-only summary presented before an import.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportPreview {
    /// Number of source items.
    pub item_count: usize,
    /// Number of source folders.
    pub folder_count: usize,
    /// Likely duplicate pairs.
    pub duplicates: Vec<DuplicateMatch>,
}

/// Result of an optional secure source-file cleanup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SecureDeleteReport {
    /// Number of bytes overwritten before removal.
    pub bytes_overwritten: u64,
}

/// Post-commit source cleanup state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportCleanup {
    /// Cleanup was not requested.
    NotRequested,
    /// Source was overwritten and removed.
    Completed(SecureDeleteReport),
    /// Import committed, but source cleanup failed.
    Failed(ImportIssue),
}

/// Committed import counts and follow-up information.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportReport {
    /// Number of inserted items.
    pub imported: usize,
    /// Number skipped by duplicate policy.
    pub skipped: usize,
    /// Number of imported items linked to `Dev Local` by routing.
    pub routed: usize,
    /// Parser and cleanup warnings.
    pub issues: Vec<ImportIssue>,
    /// Source cleanup outcome.
    pub cleanup: ImportCleanup,
}
