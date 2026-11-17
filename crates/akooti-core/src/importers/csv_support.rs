use std::collections::BTreeMap;
use std::io::Cursor;

use encoding_rs::{UTF_16BE, UTF_16LE, WINDOWS_1252};

use super::error::ImportError;
use super::types::{ImportIssue, ImportIssueKind};

pub(crate) struct CsvDocument {
    pub(crate) rows: Vec<CsvRow>,
    pub(crate) issues: Vec<ImportIssue>,
}

pub(crate) struct CsvRow {
    pub(crate) line: Option<usize>,
    values: BTreeMap<String, String>,
}

impl CsvRow {
    pub(crate) fn value(&self, names: &[&str]) -> Option<&str> {
        names
            .iter()
            .find_map(|name| self.values.get(&normalize_header(name)))
            .map(String::as_str)
            .filter(|value| !value.trim().is_empty())
    }

    pub(crate) fn entries(&self) -> impl Iterator<Item = (&str, &str)> {
        self.values
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str()))
    }
}

pub(crate) fn decode_text(bytes: &[u8]) -> String {
    if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8_lossy(rest).into_owned();
    }
    if let Some(rest) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        return UTF_16LE.decode(rest).0.into_owned();
    }
    if let Some(rest) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        return UTF_16BE.decode(rest).0.into_owned();
    }
    match std::str::from_utf8(bytes) {
        Ok(text) => text.to_owned(),
        Err(_) => WINDOWS_1252.decode(bytes).0.into_owned(),
    }
}

pub(crate) fn parse_csv(bytes: &[u8]) -> Result<CsvDocument, ImportError> {
    let text = decode_text(bytes);
    let delimiter = detect_delimiter(&text);
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(delimiter)
        .has_headers(true)
        .flexible(true)
        .from_reader(Cursor::new(text.as_bytes()));
    let headers = reader
        .headers()?
        .iter()
        .map(normalize_header)
        .collect::<Vec<_>>();
    let mut rows = Vec::new();
    let mut issues = Vec::new();
    for (index, record) in reader.records().enumerate() {
        match record {
            Ok(record) => {
                if record.iter().all(|value| value.trim().is_empty()) {
                    continue;
                }
                let mut values = BTreeMap::new();
                for (header, value) in headers.iter().zip(record.iter()) {
                    if !header.is_empty() {
                        values.insert(header.clone(), value.trim().to_owned());
                    }
                }
                let line = record.position().map(|position| position.line() as usize);
                rows.push(CsvRow { line, values });
            }
            Err(_) => issues.push(ImportIssue {
                line: Some(index.saturating_add(2)),
                kind: ImportIssueKind::InvalidRow,
            }),
        }
    }
    Ok(CsvDocument { rows, issues })
}

pub(crate) fn normalize_header(value: &str) -> String {
    value
        .trim_start_matches('\u{feff}')
        .trim()
        .to_ascii_lowercase()
}

fn detect_delimiter(text: &str) -> u8 {
    let header = text.lines().next().unwrap_or_default();
    let semicolon = header.bytes().filter(|byte| *byte == b';').count();
    let comma = header.bytes().filter(|byte| *byte == b',').count();
    let tab = header.bytes().filter(|byte| *byte == b'\t').count();
    if semicolon > comma && semicolon >= tab {
        b';'
    } else if tab > comma {
        b'\t'
    } else {
        b','
    }
}
