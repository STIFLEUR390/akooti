use regex::Regex;

use crate::error::VaultError;
use crate::models::{CustomFieldInput, ItemUriInput, Selectors, UriMatchType};

const SELECTOR_MAX_CHARS: usize = 256;
const URI_MAX_CHARS: usize = 2_048;
const FIELD_NAME_MAX_CHARS: usize = 128;

pub(crate) fn validate_selectors(selectors: &Selectors) -> Result<(), VaultError> {
    for selector in [
        selectors.username.as_deref(),
        selectors.password.as_deref(),
        selectors.totp.as_deref(),
        selectors.submit.as_deref(),
    ]
    .into_iter()
    .flatten()
    {
        if selector.is_empty()
            || selector.chars().count() > SELECTOR_MAX_CHARS
            || selector.chars().any(char::is_control)
        {
            return Err(VaultError::InvalidInput(
                "item selector is invalid or too long",
            ));
        }
    }
    Ok(())
}

pub(crate) fn validate_uris(uris: &[ItemUriInput]) -> Result<(), VaultError> {
    for uri in uris {
        if uri.uri.is_empty()
            || uri.uri.chars().count() > URI_MAX_CHARS
            || uri.uri.chars().any(char::is_control)
        {
            return Err(VaultError::InvalidInput("item URI is invalid or too long"));
        }
        if uri.match_type == UriMatchType::Regex {
            Regex::new(&uri.uri)
                .map_err(|_| VaultError::InvalidInput("item URI regex is invalid"))?;
        }
    }
    Ok(())
}

pub(crate) fn validate_custom_fields(fields: &[CustomFieldInput]) -> Result<(), VaultError> {
    for field in fields {
        if field.field_name.trim().is_empty()
            || field.field_name.chars().count() > FIELD_NAME_MAX_CHARS
            || field.field_name.chars().any(char::is_control)
        {
            return Err(VaultError::InvalidInput(
                "custom field name is invalid or too long",
            ));
        }
    }
    Ok(())
}
