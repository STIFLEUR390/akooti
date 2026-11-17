//! Serializable payloads exchanged between the Rust commands and the React UI.
//!
//! Every type here mirrors a `packages/shared/src/models.ts` interface. None of
//! them carries a plaintext secret: [`RevealedSecretDto`] is only produced by an
//! explicit `reveal_secret` invocation and always carries an expiry.

use akooti_core::{
    CustomField, CustomFieldInput, DuplicateMatch, DuplicatePolicy, DuplicateReason, Folder,
    FolderDraft, ImportCleanup, ImportIssue, ImportIssueKind, ImportOptions, ImportPreview,
    ImportReport, ItemMetadataDraft, ItemType, ItemUri, ItemUriInput, NewRoutingRule,
    RevealedSecret, RoutingPatternType, RoutingRule, SecretField, SecretString, Selectors,
    UriMatchType, VaultItem, VaultItemDraft,
};
use serde::{Deserialize, Serialize};

/// Seconds a revealed secret stays usable in the UI.
pub const REVEAL_TTL_SECONDS: u64 = 30;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderDto {
    pub id: String,
    pub parent_id: Option<String>,
    pub name: String,
    pub slug: Option<String>,
    pub icon: String,
    pub color: Option<String>,
    pub is_system: bool,
    pub require_biometric_for_mcp: bool,
    pub created_at: i64,
    pub updated_at: i64,
    pub deleted_at: Option<i64>,
}

impl From<Folder> for FolderDto {
    fn from(folder: Folder) -> Self {
        Self {
            id: folder.id,
            parent_id: folder.parent_id,
            name: folder.name,
            slug: folder.slug,
            icon: folder.icon,
            color: folder.color,
            is_system: folder.is_system,
            require_biometric_for_mcp: folder.require_biometric_for_mcp,
            created_at: folder.created_at,
            updated_at: folder.updated_at,
            deleted_at: folder.deleted_at,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderDraftDto {
    pub name: String,
    #[serde(default)]
    pub parent_id: Option<String>,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub require_biometric_for_mcp: bool,
}

impl From<FolderDraftDto> for FolderDraft {
    fn from(input: FolderDraftDto) -> Self {
        Self {
            parent_id: input.parent_id,
            name: input.name,
            icon: input.icon.unwrap_or_else(|| "folder".to_owned()),
            color: input.color,
            require_biometric_for_mcp: input.require_biometric_for_mcp,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemUriDto {
    pub id: String,
    pub uri: String,
    pub match_type: String,
}

impl From<ItemUri> for ItemUriDto {
    fn from(uri: ItemUri) -> Self {
        Self {
            id: uri.id,
            uri: uri.uri,
            match_type: match_type_name(uri.match_type).to_owned(),
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemUriInputDto {
    pub uri: String,
    #[serde(default)]
    pub match_type: Option<String>,
}

impl ItemUriInputDto {
    /// Converts to the core input, rejecting unknown match types.
    ///
    /// # Errors
    ///
    /// Returns the offending value when `match_type` is not supported.
    pub fn into_core(self) -> Result<ItemUriInput, String> {
        let match_type = self
            .match_type
            .as_deref()
            .map_or(Ok(UriMatchType::Domain), parse_match_type)?;
        Ok(ItemUriInput {
            uri: self.uri,
            match_type,
        })
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomFieldDto {
    pub id: String,
    pub field_name: String,
    pub is_secret: bool,
    pub sort_order: i64,
}

impl From<CustomField> for CustomFieldDto {
    fn from(field: CustomField) -> Self {
        Self {
            id: field.id,
            field_name: field.field_name,
            is_secret: field.is_secret,
            sort_order: field.sort_order,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomFieldInputDto {
    pub field_name: String,
    /// Plaintext value; dropped as soon as it is encrypted.
    pub value: String,
    #[serde(default = "default_true")]
    pub is_secret: bool,
    #[serde(default)]
    pub sort_order: i64,
}

impl From<CustomFieldInputDto> for CustomFieldInput {
    fn from(input: CustomFieldInputDto) -> Self {
        Self {
            field_name: input.field_name,
            value: SecretString::new(input.value),
            is_secret: input.is_secret,
            sort_order: input.sort_order,
        }
    }
}

const fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectorsDto {
    pub username: Option<String>,
    pub password: Option<String>,
    pub totp: Option<String>,
    pub submit: Option<String>,
}

impl From<Selectors> for SelectorsDto {
    fn from(selectors: Selectors) -> Self {
        Self {
            username: selectors.username,
            password: selectors.password,
            totp: selectors.totp,
            submit: selectors.submit,
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectorsInput {
    pub username: Option<String>,
    pub password: Option<String>,
    pub totp: Option<String>,
    pub submit: Option<String>,
}

impl From<SelectorsInput> for Selectors {
    fn from(input: SelectorsInput) -> Self {
        Self {
            username: non_empty(input.username),
            password: non_empty(input.password),
            totp: non_empty(input.totp),
            submit: non_empty(input.submit),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultItemDto {
    pub id: String,
    pub item_type: String,
    pub title: String,
    pub primary_url: Option<String>,
    pub username: Option<String>,
    pub selectors: SelectorsDto,
    pub zxcvbn_score: Option<i64>,
    pub is_compromised: bool,
    pub password_updated_at: Option<i64>,
    pub created_by_agent: Option<String>,
    pub expires_at: Option<i64>,
    pub favorite: bool,
    pub folder_ids: Vec<String>,
    pub uris: Vec<ItemUriDto>,
    pub custom_fields: Vec<CustomFieldDto>,
    pub created_at: i64,
    pub updated_at: i64,
    pub deleted_at: Option<i64>,
    /// Presence flag: an encrypted password exists.
    pub has_password: bool,
    /// Presence flag: an encrypted TOTP seed exists.
    pub has_totp: bool,
    /// Presence flag: encrypted notes exist.
    pub has_notes: bool,
}

impl From<VaultItem> for VaultItemDto {
    fn from(item: VaultItem) -> Self {
        Self {
            id: item.id,
            item_type: item_type_name(item.item_type).to_owned(),
            title: item.title,
            primary_url: item.primary_url,
            username: item.username,
            selectors: item.selectors.into(),
            zxcvbn_score: item.zxcvbn_score,
            is_compromised: item.is_compromised,
            password_updated_at: item.password_updated_at,
            created_by_agent: item.created_by_agent,
            expires_at: item.expires_at,
            favorite: item.favorite,
            folder_ids: item.folder_ids,
            uris: item.uris.into_iter().map(Into::into).collect(),
            custom_fields: item.custom_fields.into_iter().map(Into::into).collect(),
            created_at: item.created_at,
            updated_at: item.updated_at,
            deleted_at: item.deleted_at,
            has_password: item.has_password,
            has_totp: item.has_totp,
            has_notes: item.has_notes,
        }
    }
}

/// Item creation payload. Secrets are optional and never echoed back.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateItemInput {
    pub item_type: String,
    pub title: String,
    #[serde(default)]
    pub primary_url: Option<String>,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub totp_secret: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub selectors: SelectorsInput,
    #[serde(default)]
    pub favorite: bool,
    #[serde(default)]
    pub expires_at: Option<i64>,
    #[serde(default)]
    pub created_by_agent: Option<String>,
    #[serde(default)]
    pub folder_ids: Vec<String>,
    #[serde(default)]
    pub uris: Vec<ItemUriInputDto>,
    #[serde(default)]
    pub custom_fields: Vec<CustomFieldInputDto>,
}

impl CreateItemInput {
    /// Converts to a core draft.
    ///
    /// # Errors
    ///
    /// Returns the offending value for an unsupported item type or URI match.
    pub fn into_core(self) -> Result<VaultItemDraft, String> {
        Ok(VaultItemDraft {
            item_type: parse_item_type(&self.item_type)?,
            title: self.title,
            primary_url: non_empty(self.primary_url),
            username: non_empty(self.username),
            password: self.password.map(SecretString::new),
            totp_secret: self.totp_secret.map(SecretString::new),
            notes: self.notes.map(SecretString::new),
            selectors: self.selectors.into(),
            created_by_agent: self.created_by_agent,
            expires_at: self.expires_at,
            favorite: self.favorite,
            folder_ids: self.folder_ids,
            uris: convert_uris(self.uris)?,
            custom_fields: self.custom_fields.into_iter().map(Into::into).collect(),
        })
    }
}

/// Metadata-only update payload; secrets are written separately.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateItemInput {
    pub id: String,
    pub item_type: String,
    pub title: String,
    #[serde(default)]
    pub primary_url: Option<String>,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub selectors: SelectorsInput,
    #[serde(default)]
    pub favorite: bool,
    #[serde(default)]
    pub expires_at: Option<i64>,
    #[serde(default)]
    pub folder_ids: Vec<String>,
    #[serde(default)]
    pub uris: Vec<ItemUriInputDto>,
}

impl UpdateItemInput {
    /// Converts to a core metadata draft.
    ///
    /// # Errors
    ///
    /// Returns the offending value for an unsupported item type or URI match.
    pub fn into_core(self) -> Result<ItemMetadataDraft, String> {
        Ok(ItemMetadataDraft {
            item_type: parse_item_type(&self.item_type)?,
            title: self.title,
            primary_url: non_empty(self.primary_url),
            username: non_empty(self.username),
            selectors: self.selectors.into(),
            expires_at: self.expires_at,
            favorite: self.favorite,
            folder_ids: self.folder_ids,
            uris: convert_uris(self.uris)?,
        })
    }
}

/// A secret revealed to the UI after an explicit user action.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RevealedSecretDto {
    pub item_id: String,
    pub field: String,
    /// Plaintext value. Dropped by the UI at `expires_at`.
    pub value: String,
    /// Epoch milliseconds after which the UI must hide the value again.
    pub expires_at: u64,
}

impl RevealedSecretDto {
    /// Wraps a core reveal result with a UI expiry.
    #[must_use]
    pub fn new(secret: RevealedSecret, now_millis: u64) -> Self {
        Self {
            item_id: secret.item_id,
            field: secret.field.as_str().to_owned(),
            value: secret.value.expose().to_owned(),
            expires_at: now_millis + REVEAL_TTL_SECONDS * 1_000,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutingRuleDto {
    pub id: String,
    pub pattern: String,
    pub pattern_type: String,
    pub target_folder_id: String,
    pub is_default: bool,
    pub enabled: bool,
    pub created_at: i64,
}

impl From<RoutingRule> for RoutingRuleDto {
    fn from(rule: RoutingRule) -> Self {
        Self {
            id: rule.id,
            pattern: rule.pattern,
            pattern_type: pattern_type_name(rule.pattern_type).to_owned(),
            target_folder_id: rule.target_folder_id,
            is_default: rule.is_default,
            enabled: rule.enabled,
            created_at: rule.created_at,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewRoutingRuleInput {
    pub pattern: String,
    #[serde(default)]
    pub pattern_type: Option<String>,
    pub target_folder_id: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

impl NewRoutingRuleInput {
    /// Converts to the core input.
    ///
    /// # Errors
    ///
    /// Returns the offending value for an unsupported pattern type.
    pub fn into_core(self) -> Result<NewRoutingRule, String> {
        let pattern_type = self
            .pattern_type
            .as_deref()
            .map_or(Ok(RoutingPatternType::Suffix), parse_pattern_type)?;
        Ok(NewRoutingRule {
            pattern: self.pattern,
            pattern_type,
            target_folder_id: self.target_folder_id,
            enabled: self.enabled,
        })
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportIssueDto {
    pub kind: String,
    pub detail: String,
}

impl From<ImportIssue> for ImportIssueDto {
    fn from(issue: ImportIssue) -> Self {
        Self {
            kind: issue_kind_name(&issue.kind).to_owned(),
            detail: match issue.line {
                Some(line) => format!("ligne {line}"),
                None => String::new(),
            },
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateMatchDto {
    pub imported_external_id: String,
    pub existing_item_id: String,
    pub reason: String,
    pub score: f64,
}

impl From<DuplicateMatch> for DuplicateMatchDto {
    fn from(duplicate: DuplicateMatch) -> Self {
        Self {
            imported_external_id: duplicate.imported_external_id,
            existing_item_id: duplicate.existing_item_id,
            reason: duplicate_reason_name(duplicate.reason).to_owned(),
            score: duplicate.score,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportPreviewDto {
    pub item_count: usize,
    pub folder_count: usize,
    pub duplicates: Vec<DuplicateMatchDto>,
}

impl From<ImportPreview> for ImportPreviewDto {
    fn from(preview: ImportPreview) -> Self {
        Self {
            item_count: preview.item_count,
            folder_count: preview.folder_count,
            duplicates: preview.duplicates.into_iter().map(Into::into).collect(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportReportDto {
    pub imported: usize,
    pub skipped: usize,
    pub routed: usize,
    pub issues: Vec<ImportIssueDto>,
    pub cleanup: String,
}

impl From<ImportReport> for ImportReportDto {
    fn from(report: ImportReport) -> Self {
        let cleanup = match &report.cleanup {
            ImportCleanup::NotRequested => "not_requested",
            ImportCleanup::Completed(_) => "completed",
            ImportCleanup::Failed(_) => "failed",
        };
        Self {
            imported: report.imported,
            skipped: report.skipped,
            routed: report.routed,
            issues: report.issues.into_iter().map(Into::into).collect(),
            cleanup: cleanup.to_owned(),
        }
    }
}

/// Applies the UI import options to a core report request.
#[must_use]
pub fn import_options(
    route_dev_local: bool,
    import_anyway: bool,
    secure_delete_source: bool,
) -> ImportOptions {
    ImportOptions {
        route_dev_local,
        duplicate_policy: if import_anyway {
            DuplicatePolicy::ImportAnyway
        } else {
            DuplicatePolicy::Skip
        },
        secure_delete_source,
    }
}

fn convert_uris(uris: Vec<ItemUriInputDto>) -> Result<Vec<ItemUriInput>, String> {
    uris.into_iter().map(ItemUriInputDto::into_core).collect()
}

fn non_empty(value: Option<String>) -> Option<String> {
    value.filter(|value| !value.trim().is_empty())
}

fn parse_item_type(value: &str) -> Result<ItemType, String> {
    match value {
        "login" => Ok(ItemType::Login),
        "api_key" | "apiKey" => Ok(ItemType::ApiKey),
        "note" => Ok(ItemType::Note),
        "env_set" | "envSet" => Ok(ItemType::EnvSet),
        other => Err(format!("unsupported item type: {other}")),
    }
}

fn parse_match_type(value: &str) -> Result<UriMatchType, String> {
    match value {
        "domain" => Ok(UriMatchType::Domain),
        "host" => Ok(UriMatchType::Host),
        "starts_with" | "startsWith" => Ok(UriMatchType::StartsWith),
        "exact" => Ok(UriMatchType::Exact),
        "regex" => Ok(UriMatchType::Regex),
        other => Err(format!("unsupported URI match type: {other}")),
    }
}

fn parse_pattern_type(value: &str) -> Result<RoutingPatternType, String> {
    match value {
        "suffix" => Ok(RoutingPatternType::Suffix),
        "exact_host" | "exactHost" => Ok(RoutingPatternType::ExactHost),
        "cidr" => Ok(RoutingPatternType::Cidr),
        "regex" => Ok(RoutingPatternType::Regex),
        other => Err(format!("unsupported routing pattern type: {other}")),
    }
}

const fn item_type_name(item_type: ItemType) -> &'static str {
    match item_type {
        ItemType::Login => "login",
        ItemType::ApiKey => "api_key",
        ItemType::Note => "note",
        ItemType::EnvSet => "env_set",
    }
}

const fn match_type_name(match_type: UriMatchType) -> &'static str {
    match match_type {
        UriMatchType::Domain => "domain",
        UriMatchType::Host => "host",
        UriMatchType::StartsWith => "starts_with",
        UriMatchType::Exact => "exact",
        UriMatchType::Regex => "regex",
    }
}

const fn pattern_type_name(pattern_type: RoutingPatternType) -> &'static str {
    match pattern_type {
        RoutingPatternType::Suffix => "suffix",
        RoutingPatternType::ExactHost => "exact_host",
        RoutingPatternType::Cidr => "cidr",
        RoutingPatternType::Regex => "regex",
    }
}

const fn duplicate_reason_name(reason: DuplicateReason) -> &'static str {
    match reason {
        DuplicateReason::UrlAndUsername => "url_and_username",
        DuplicateReason::FuzzyTitle => "fuzzy_title",
    }
}

const fn issue_kind_name(kind: &ImportIssueKind) -> &'static str {
    match kind {
        ImportIssueKind::InvalidRow => "invalid_row",
        ImportIssueKind::MissingTitle => "missing_title",
        ImportIssueKind::UnsupportedItem => "unsupported_item",
        ImportIssueKind::ProtectedXmlValue => "protected_xml_value",
        ImportIssueKind::InvalidTotp => "invalid_totp",
        ImportIssueKind::InvalidFolder => "invalid_folder",
        ImportIssueKind::SecureDeleteFailed => "secure_delete_failed",
        _ => "unknown",
    }
}

/// Stable name for a secret field requested by the UI.
///
/// # Errors
///
/// Returns the offending value for an unsupported field name.
pub fn parse_secret_field(value: &str) -> Result<SecretField, String> {
    match value {
        "password" => Ok(SecretField::Password),
        "totp" => Ok(SecretField::Totp),
        "notes" => Ok(SecretField::Notes),
        "custom_field" | "customField" => Ok(SecretField::CustomField),
        other => Err(format!("unsupported secret field: {other}")),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CreateItemInput, ItemUriInputDto, NewRoutingRuleInput, REVEAL_TTL_SECONDS,
        RevealedSecretDto, UpdateItemInput, non_empty, parse_secret_field,
    };
    use akooti_core::{RevealedSecret, SecretField, SecretString};

    #[test]
    fn reveal_dto_carries_a_bounded_expiry() {
        let revealed = RevealedSecret {
            item_id: "item".to_owned(),
            field: SecretField::Password,
            value: SecretString::new("s3cret".to_owned()),
        };
        let dto = RevealedSecretDto::new(revealed, 1_000);
        assert_eq!(dto.field, "password");
        assert_eq!(dto.expires_at, 1_000 + REVEAL_TTL_SECONDS * 1_000);
    }

    fn create_input(item_type: &str) -> CreateItemInput {
        CreateItemInput {
            item_type: item_type.to_owned(),
            title: "Title".to_owned(),
            folder_ids: vec!["folder".to_owned()],
            ..CreateItemInput::default()
        }
    }

    #[test]
    fn create_input_rejects_unknown_enums() {
        assert!(create_input("login").into_core().is_ok());
        assert!(create_input("api_key").into_core().is_ok());
        assert!(create_input("unicorn").into_core().is_err());
    }

    #[test]
    fn blank_optional_fields_become_none() {
        assert_eq!(non_empty(Some("  ".to_owned())), None);
        assert_eq!(
            non_empty(Some("value".to_owned())),
            Some("value".to_owned())
        );
        assert_eq!(non_empty(None), None);
    }

    #[test]
    fn uri_inputs_default_to_domain_matching() {
        let uri = ItemUriInputDto {
            uri: "https://example.test".to_owned(),
            match_type: None,
        };
        assert_eq!(
            uri.into_core().expect("uri").match_type,
            akooti_core::UriMatchType::Domain
        );
        assert!(
            ItemUriInputDto {
                uri: "x".to_owned(),
                match_type: Some("nope".to_owned()),
            }
            .into_core()
            .is_err()
        );
    }

    #[test]
    fn routing_and_secret_parsers_reject_unknown_values() {
        assert_eq!(
            parse_secret_field("notes").expect("notes"),
            SecretField::Notes
        );
        assert!(parse_secret_field("passkey").is_err());
        assert!(
            NewRoutingRuleInput {
                pattern: ".test".to_owned(),
                pattern_type: Some("wildcard".to_owned()),
                target_folder_id: "folder".to_owned(),
                enabled: true,
            }
            .into_core()
            .is_err()
        );
    }

    #[test]
    fn update_input_keeps_metadata_only() {
        let input = UpdateItemInput {
            id: "item".to_owned(),
            item_type: "note".to_owned(),
            title: "Note".to_owned(),
            primary_url: None,
            username: None,
            selectors: super::SelectorsInput::default(),
            favorite: true,
            expires_at: None,
            folder_ids: vec!["folder".to_owned()],
            uris: Vec::new(),
        };
        let draft = input.into_core().expect("draft");
        assert!(draft.favorite);
        assert_eq!(draft.folder_ids, vec!["folder".to_owned()]);
    }
}
