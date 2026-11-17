use std::io::{Cursor, Read};

use zeroize::Zeroizing;

use super::error::ImportError;

pub(crate) fn read_zip_entry(
    bytes: &[u8],
    suffix: &str,
) -> Result<Zeroizing<Vec<u8>>, ImportError> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))?;
    let mut selected = None;
    for index in 0..archive.len() {
        let entry = archive.by_index(index)?;
        if entry.name()?.to_ascii_lowercase().ends_with(suffix) {
            selected = Some(index);
            break;
        }
    }
    let index = selected.ok_or(ImportError::CorruptSource)?;
    let mut entry = archive.by_index(index)?;
    let mut output = Zeroizing::new(Vec::new());
    entry.read_to_end(output.as_mut())?;
    Ok(output)
}
