//! Sent requests, kept between runs in `history.jsonl` (one entry per line,
//! newest last). Only the request and a summary of the answer are kept:
//! response bodies can hold personal data and are never written. Before an
//! entry is written, anything that looks like a secret is replaced.

use std::io::Write;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::http_file::SavedRequest;
use crate::secret_scan::{scan, secret_in};

/// Entries kept on disk; older ones are dropped.
pub const MAX_ENTRIES: usize = 500;
/// What replaces a secret in history.
pub const HIDDEN: &str = "‹hidden›";

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    /// Milliseconds since 1970, from the UI's clock.
    pub at: u64,
    pub env_name: Option<String>,
    pub request: SavedRequest,
    pub status: Option<u16>,
    pub total_ms: Option<u64>,
    pub size: Option<u64>,
    pub error: Option<String>,
}

/// Replaces literal secrets in the request and the error text.
pub fn scrub(mut e: HistoryEntry) -> HistoryEntry {
    for f in scan(&e.request) {
        let swap = |t: &str| t.replace(&f.value, HIDDEN);
        e.request.url = swap(&e.request.url);
        e.request.body = swap(&e.request.body);
        for h in &mut e.request.headers {
            h.value = swap(&h.value);
        }
    }
    if let Some(err) = &e.error
        && let Some(s) = secret_in(err)
    {
        e.error = Some(err.replace(&s, HIDDEN));
    }
    e
}

pub fn load(path: &Path) -> Vec<HistoryEntry> {
    let Ok(text) = std::fs::read_to_string(path) else { return Vec::new() };
    // A damaged line is skipped, not fatal: history is a convenience.
    text.lines().filter_map(|l| serde_json::from_str(l).ok()).collect()
}

pub fn append(path: &Path, entry: HistoryEntry) -> Result<HistoryEntry, String> {
    let entry = scrub(entry);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let line = serde_json::to_string(&entry).map_err(|e| e.to_string())?;
    let mut opts = std::fs::OpenOptions::new();
    opts.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut file = opts.open(path).map_err(|e| e.to_string())?;
    writeln!(file, "{line}").map_err(|e| e.to_string())?;
    trim(path)?;
    Ok(entry)
}

/// Keeps the newest entries once the file holds a tenth more than allowed.
fn trim(path: &Path) -> Result<(), String> {
    let entries = load(path);
    if entries.len() <= MAX_ENTRIES + MAX_ENTRIES / 10 {
        return Ok(());
    }
    let keep = &entries[entries.len() - MAX_ENTRIES..];
    let text: String = keep.iter().filter_map(|e| serde_json::to_string(e).ok()).map(|l| l + "\n").collect();
    let tmp = path.with_extension("jsonl.tmp");
    std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
    std::fs::rename(tmp, path).map_err(|e| e.to_string())
}

pub fn clear(path: &Path) -> Result<(), String> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http_file::SavedHeader;

    fn entry(at: u64, auth: &str) -> HistoryEntry {
        HistoryEntry {
            at,
            env_name: Some("Local".into()),
            request: SavedRequest {
                name: String::new(),
                method: "GET".into(),
                url: "{{base}}/me".into(),
                headers: vec![SavedHeader { name: "Authorization".into(), value: auth.into(), enabled: true }],
                body_mode: "none".into(),
                body: String::new(),
                expect: vec![],
                save: vec![],
            },
            status: Some(200),
            total_ms: Some(84),
            size: Some(412),
            error: None,
        }
    }

    #[test]
    fn keeps_entries_between_runs_without_secrets() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.jsonl");
        append(&path, entry(1, "Bearer {{token}}")).unwrap();
        let kept = append(&path, entry(2, "Bearer abc123secret")).unwrap();
        assert_eq!(kept.request.headers[0].value, "Bearer ‹hidden›");

        let back = load(&path);
        assert_eq!(back.len(), 2);
        assert_eq!(back[0].request.headers[0].value, "Bearer {{token}}");
        assert!(!std::fs::read_to_string(&path).unwrap().contains("abc123secret"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        }
    }

    #[test]
    fn hides_secrets_in_error_text() {
        let mut e = entry(1, "");
        // Built at run time, so secret scanners do not mistake the test for a leak.
        e.error = Some(format!("Could not find api.dev?key={}", ["sk", "live", "51HxYz2eZvKYlo2C8w9a"].join("_")));
        assert_eq!(scrub(e).error.as_deref(), Some("Could not find api.dev?key=‹hidden›"));
    }

    #[test]
    fn keeps_only_the_newest() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.jsonl");
        for i in 0..(MAX_ENTRIES as u64 + MAX_ENTRIES as u64 / 10 + 1) {
            append(&path, entry(i, "")).unwrap();
        }
        let back = load(&path);
        assert_eq!(back.len(), MAX_ENTRIES);
        assert_eq!(back.last().unwrap().at, MAX_ENTRIES as u64 + MAX_ENTRIES as u64 / 10);
    }

    #[test]
    fn skips_damaged_lines_and_clears() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.jsonl");
        append(&path, entry(1, "")).unwrap();
        std::fs::OpenOptions::new().append(true).open(&path).unwrap().write_all(b"{ broken\n").unwrap();
        assert_eq!(load(&path).len(), 1);
        clear(&path).unwrap();
        assert!(load(&path).is_empty());
        clear(&path).unwrap();
    }
}
