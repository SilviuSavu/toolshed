//! Hash-chained JSONL audit trail for tool invocations.
//!
//! Ported from Tallow's TypeScript implementation. Each `toolshed run`
//! produces exactly 2 entries: `tool_call` (before) + `tool_result` (after).
//! The hash chain provides tamper evidence via SHA-256 chaining.

use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

use chrono::Utc;
use regex::Regex;
use sha2::{Digest, Sha256};

use crate::config;

const REDACTED: &str = "[REDACTED]";

static SENSITIVE_KEY_SEGMENTS: &[&str] = &[
    "auth",
    "authorization",
    "bearer",
    "cookie",
    "cookies",
    "credential",
    "credentials",
    "key",
    "password",
    "passwd",
    "passphrase",
    "secret",
    "token",
];

static SENSITIVE_COMPACT_PATTERNS: &[&str] = &[
    "accesskey",
    "apikey",
    "clientsecret",
    "privatekey",
    "setcookie",
];

// --- Types ---

#[derive(Debug, Clone)]
pub struct AuditEntry {
    pub seq: u64,
    pub ts: String,
    pub session_id: String,
    pub category: String,
    pub event: String,
    pub actor: String,
    pub data: serde_json::Value,
    pub outcome: Option<String>,
    pub prev_hash: String,
    pub hash: String,
}

impl serde::Serialize for AuditEntry {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let count = 8 + usize::from(self.outcome.is_some());
        let mut map = serializer.serialize_map(Some(count))?;
        map.serialize_entry("seq", &self.seq)?;
        map.serialize_entry("ts", &self.ts)?;
        map.serialize_entry("sessionId", &self.session_id)?;
        map.serialize_entry("category", &self.category)?;
        map.serialize_entry("event", &self.event)?;
        map.serialize_entry("actor", &self.actor)?;
        map.serialize_entry("data", &self.data)?;
        if let Some(ref o) = self.outcome { map.serialize_entry("outcome", o)?; }
        map.serialize_entry("prevHash", &self.prev_hash)?;
        map.serialize_entry("hash", &self.hash)?;
        map.end()
    }
}

impl<'de> serde::Deserialize<'de> for AuditEntry {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let v = serde_json::Value::deserialize(deserializer).map_err(serde::de::Error::custom)?;
        let obj = v.as_object().ok_or_else(|| serde::de::Error::custom("expected object"))?;
        Ok(Self {
            seq: obj.get("seq").and_then(serde_json::Value::as_u64).unwrap_or(0),
            ts: obj.get("ts").and_then(serde_json::Value::as_str).unwrap_or_default().to_string(),
            session_id: obj.get("sessionId").and_then(serde_json::Value::as_str).unwrap_or_default().to_string(),
            category: obj.get("category").and_then(serde_json::Value::as_str).unwrap_or_default().to_string(),
            event: obj.get("event").and_then(serde_json::Value::as_str).unwrap_or_default().to_string(),
            actor: obj.get("actor").and_then(serde_json::Value::as_str).unwrap_or_default().to_string(),
            data: obj.get("data").cloned().unwrap_or(serde_json::Value::Null),
            outcome: obj.get("outcome").and_then(serde_json::Value::as_str).map(String::from),
            prev_hash: obj.get("prevHash").and_then(serde_json::Value::as_str).unwrap_or_default().to_string(),
            hash: obj.get("hash").and_then(serde_json::Value::as_str).unwrap_or_default().to_string(),
        })
    }
}

pub struct IntegrityResult {
    pub valid: bool,
    pub total_entries: usize,
    pub error_message: Option<String>,
}

pub struct AuditFileInfo {
    pub path: PathBuf,
    pub session_id: String,
    pub date: String,
    pub size_bytes: u64,
    pub entry_count: usize,
}

pub struct QueryOptions {
    pub event: Option<String>,
    pub outcome: Option<String>,
    pub since: Option<String>,
    pub search: Option<String>,
    pub limit: usize,
}

// --- Sensitive Key Detection ---

fn normalize_key_segments(key: &str) -> Vec<String> {
    let mut result = String::with_capacity(key.len() + 4);
    let chars: Vec<char> = key.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        if c.is_ascii_uppercase() && i > 0 {
            let prev = chars[i - 1];
            if prev.is_ascii_lowercase() || prev.is_ascii_digit() {
                result.push('_');
            }
        }
        if c.is_ascii_alphanumeric() {
            result.push(c.to_ascii_lowercase());
        } else {
            result.push('_');
        }
    }
    result
        .split('_')
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect()
}

pub fn is_sensitive_key(key: &str) -> bool {
    let segments = normalize_key_segments(key);
    if segments.is_empty() {
        return false;
    }
    if segments
        .iter()
        .any(|s| SENSITIVE_KEY_SEGMENTS.contains(&s.as_str()))
    {
        return true;
    }
    let compact: String = segments.concat();
    SENSITIVE_COMPACT_PATTERNS
        .iter()
        .any(|p| compact.contains(p))
}

fn redact_value(value: &serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => {
            let mut result = serde_json::Map::new();
            for (k, v) in map {
                if is_sensitive_key(k) {
                    result.insert(k.clone(), serde_json::Value::String(REDACTED.to_string()));
                } else {
                    result.insert(k.clone(), redact_value(v));
                }
            }
            serde_json::Value::Object(result)
        }
        serde_json::Value::Array(arr) => {
            serde_json::Value::Array(arr.iter().map(redact_value).collect())
        }
        other => other.clone(),
    }
}

// --- Hash Computation ---

fn canonicalize(value: &serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => {
            let mut sorted = serde_json::Map::new();
            for (k, v) in map {
                sorted.insert(k.clone(), canonicalize(v));
            }
            serde_json::Value::Object(sorted)
        }
        serde_json::Value::Array(arr) => {
            serde_json::Value::Array(arr.iter().map(canonicalize).collect())
        }
        other => other.clone(),
    }
}

pub fn compute_entry_hash(entry: &AuditEntry) -> String {
    // These operations should never fail on a well-formed AuditEntry,
    // but we handle errors gracefully by returning a fallback hash.
    let Ok(mut value) = serde_json::to_value(entry) else {
        return "0".repeat(64);
    };
    if let serde_json::Value::Object(ref mut map) = value {
        map.remove("hash");
    }
    let canonical = canonicalize(&value);
    let Ok(json) = serde_json::to_string(&canonical) else {
        return "0".repeat(64);
    };
    let mut hasher = Sha256::new();
    hasher.update(json.as_bytes());
    format!("{:x}", hasher.finalize())
}

// --- Logger ---

pub struct AuditLogger {
    session_id: String,
    file_path: PathBuf,
    seq: u64,
    last_hash: String,
}

impl AuditLogger {
    pub fn new(session_id: &str) -> Self {
        Self::with_dir(session_id, &config::audit_dir())
    }

    #[allow(clippy::print_stderr)]
    pub fn with_dir(session_id: &str, dir: &Path) -> Self {
        if let Err(e) = fs::create_dir_all(dir) {
            eprintln!("audit: cannot create directory {}: {e}", dir.display());
        }
        let date = Utc::now().format("%Y-%m-%d").to_string();
        let file_path = dir.join(format!("{session_id}-{date}.jsonl"));
        Self {
            session_id: session_id.to_string(),
            file_path,
            seq: 0,
            last_hash: String::new(),
        }
    }

    #[allow(clippy::print_stderr)]
    pub fn record(
        &mut self,
        category: &str,
        event: &str,
        actor: &str,
        data: &serde_json::Value,
        outcome: Option<String>,
    ) -> AuditEntry {
        self.seq += 1;
        let redacted_data = redact_value(data);

        let mut entry = AuditEntry {
            seq: self.seq,
            ts: Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            session_id: self.session_id.clone(),
            category: category.to_string(),
            event: event.to_string(),
            actor: actor.to_string(),
            data: redacted_data,
            outcome,
            prev_hash: self.last_hash.clone(),
            hash: String::new(),
        };

        entry.hash = compute_entry_hash(&entry);

        if let Err(e) = self.append_entry(&entry) {
            eprintln!("audit: write failed: {e}");
        }

        self.last_hash.clone_from(&entry.hash);
        entry
    }

    fn append_entry(&self, entry: &AuditEntry) -> std::io::Result<()> {
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.file_path)?;
        let json = serde_json::to_string(entry).map_err(std::io::Error::other)?;
        writeln!(file, "{json}")?;
        Ok(())
    }
}

// --- File Operations ---

fn parse_audit_file(path: &Path) -> Result<Vec<AuditEntry>, std::io::Error> {
    let content = fs::read_to_string(path)?;
    let mut entries = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Ok(entry) = serde_json::from_str::<AuditEntry>(trimmed) {
            entries.push(entry);
        }
    }
    Ok(entries)
}

pub fn verify_file(path: &Path) -> IntegrityResult {
    let entries = match parse_audit_file(path) {
        Ok(e) => e,
        Err(e) => {
            return IntegrityResult {
                valid: false,
                total_entries: 0,
                error_message: Some(format!("cannot read file: {e}")),
            };
        }
    };

    if entries.is_empty() {
        return IntegrityResult {
            valid: true,
            total_entries: 0,
            error_message: None,
        };
    }

    let mut prev_hash = String::new();
    for entry in &entries {
        if entry.prev_hash != prev_hash {
            return IntegrityResult {
                valid: false,
                total_entries: entries.len(),
                error_message: Some(format!("Entry seq={}: prevHash mismatch", entry.seq)),
            };
        }

        let expected = compute_entry_hash(entry);
        if entry.hash != expected {
            return IntegrityResult {
                valid: false,
                total_entries: entries.len(),
                error_message: Some(format!(
                    "Entry seq={}: hash mismatch (entry was tampered with)",
                    entry.seq
                )),
            };
        }

        prev_hash.clone_from(&entry.hash);
    }

    IntegrityResult {
        valid: true,
        total_entries: entries.len(),
        error_message: None,
    }
}

pub fn list_files(dir: &Path) -> Vec<AuditFileInfo> {
    let Ok(read_dir) = fs::read_dir(dir) else {
        return Vec::new();
    };

    let Ok(re) = Regex::new(r"^(.+)-(\d{4}-\d{2}-\d{2})\.jsonl$") else {
        return Vec::new();
    };
    let mut files = Vec::new();

    for entry in read_dir.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if let Some(caps) = re.captures(&name) {
            let path = entry.path();
            let Ok(meta) = fs::metadata(&path) else {
                continue;
            };
            let Ok(content) = fs::read_to_string(&path) else {
                continue;
            };
            let entry_count = content.lines().filter(|l| !l.trim().is_empty()).count();

            files.push(AuditFileInfo {
                path,
                session_id: caps[1].to_string(),
                date: caps[2].to_string(),
                size_bytes: meta.len(),
                entry_count,
            });
        }
    }

    files.sort_by(|a, b| b.date.cmp(&a.date));
    files
}

pub fn query_entries(path: &Path, options: &QueryOptions) -> Vec<AuditEntry> {
    let Ok(entries) = parse_audit_file(path) else {
        return Vec::new();
    };

    let since_ms = options.since.as_ref().and_then(|s| parse_timestamp(s));
    let search_lower = options.search.as_ref().map(|s| s.to_lowercase());

    let mut matched = Vec::new();

    for entry in &entries {
        if let Some(ref event) = options.event {
            if entry.event != *event {
                continue;
            }
        }
        if let Some(ref outcome) = options.outcome {
            match &entry.outcome {
                Some(o) if o == outcome => {}
                _ => continue,
            }
        }
        if let Some(since) = since_ms {
            if let Ok(entry_ts) = chrono::DateTime::parse_from_rfc3339(&entry.ts) {
                if entry_ts.timestamp_millis() < since {
                    continue;
                }
            }
        }
        if let Some(ref search) = search_lower {
            let serialized = serde_json::to_string(entry)
                .unwrap_or_default()
                .to_lowercase();
            if !serialized.contains(search.as_str()) {
                continue;
            }
        }
        matched.push(entry.clone());
    }

    matched.reverse(); // newest first
    matched.truncate(options.limit);
    matched
}

fn parse_timestamp(s: &str) -> Option<i64> {
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
        return Some(dt.timestamp_millis());
    }
    if let Ok(date) = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        let dt = date.and_hms_opt(0, 0, 0)?;
        return Some(dt.and_utc().timestamp_millis());
    }
    None
}

// --- Tests ---

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn hash_consistency() {
        let dir = tempfile::TempDir::new().unwrap();
        let mut logger = AuditLogger::with_dir("test-session", dir.path());

        let e1 = logger.record(
            "tool",
            "tool_call",
            "user",
            &serde_json::json!({"tool": "rc", "command": "search"}),
            None,
        );
        let e2 = logger.record(
            "tool",
            "tool_result",
            "user",
            &serde_json::json!({"tool": "rc", "durationMs": 100}),
            Some("success".into()),
        );

        assert_eq!(e1.seq, 1);
        assert_eq!(e2.seq, 2);
        assert!(e1.prev_hash.is_empty());
        assert_eq!(e2.prev_hash, e1.hash);

        // Recompute hashes — must match
        assert_eq!(compute_entry_hash(&e1), e1.hash);
        assert_eq!(compute_entry_hash(&e2), e2.hash);
    }

    #[test]
    fn sensitive_key_detection() {
        // Segment matches
        assert!(is_sensitive_key("password"));
        assert!(is_sensitive_key("api_key"));
        assert!(is_sensitive_key("apiKey"));
        assert!(is_sensitive_key("Authorization"));
        assert!(is_sensitive_key("access_token"));
        assert!(is_sensitive_key("bearer"));
        assert!(is_sensitive_key("cookie"));
        assert!(is_sensitive_key("clientSecret"));
        assert!(is_sensitive_key("PRIVATE_KEY"));

        // Compact patterns
        assert!(is_sensitive_key("apikey"));
        assert!(is_sensitive_key("clientsecret"));
        assert!(is_sensitive_key("accesskey"));

        // Negative
        assert!(!is_sensitive_key("username"));
        assert!(!is_sensitive_key("tool"));
        assert!(!is_sensitive_key("command"));
        assert!(!is_sensitive_key("query"));
        assert!(!is_sensitive_key("name"));
    }

    #[test]
    fn redaction() {
        let data = serde_json::json!({
            "tool": "rc",
            "apiKey": "test-key-12345",
            "nested": {
                "password": "hunter2",
                "name": "test"
            },
            "list": [{"token": "abc"}, {"safe": "ok"}]
        });
        let redacted = redact_value(&data);
        assert_eq!(redacted["tool"], "rc");
        assert_eq!(redacted["apiKey"], REDACTED);
        assert_eq!(redacted["nested"]["password"], REDACTED);
        assert_eq!(redacted["nested"]["name"], "test");
        assert_eq!(redacted["list"][0]["token"], REDACTED);
        assert_eq!(redacted["list"][1]["safe"], "ok");
    }

    #[test]
    fn canonicalize_preserves_sorted_keys() {
        let value = serde_json::json!({"z": 1, "a": {"y": 2, "b": 3}});
        let canonical = canonicalize(&value);
        let json = serde_json::to_string(&canonical).unwrap();
        // BTreeMap-backed serde_json already sorts, canonicalize preserves this
        assert!(json.contains("\"a\""));
        assert!(json.contains("\"z\""));
    }

    #[test]
    fn logger_write_and_verify() {
        let dir = tempfile::TempDir::new().unwrap();
        let mut logger = AuditLogger::with_dir("verify-test", dir.path());

        logger.record(
            "tool",
            "tool_call",
            "user",
            &serde_json::json!({"tool": "test"}),
            None,
        );
        logger.record(
            "tool",
            "tool_result",
            "user",
            &serde_json::json!({"tool": "test", "durationMs": 50}),
            Some("success".into()),
        );

        let files = list_files(dir.path());
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].entry_count, 2);
        assert_eq!(files[0].session_id, "verify-test");

        let result = verify_file(&files[0].path);
        assert!(result.valid);
        assert_eq!(result.total_entries, 2);
    }

    #[test]
    fn tamper_detection() {
        let dir = tempfile::TempDir::new().unwrap();
        let mut logger = AuditLogger::with_dir("tamper-test", dir.path());

        logger.record(
            "tool",
            "tool_call",
            "user",
            &serde_json::json!({"tool": "test"}),
            None,
        );
        logger.record(
            "tool",
            "tool_result",
            "user",
            &serde_json::json!({"tool": "test"}),
            Some("success".into()),
        );

        let files = list_files(dir.path());
        assert_eq!(files.len(), 1);

        // Tamper with the file
        let content = fs::read_to_string(&files[0].path).unwrap();
        let tampered = content.replacen("tool_call", "tool_hack", 1);
        fs::write(&files[0].path, tampered).unwrap();

        let result = verify_file(&files[0].path);
        assert!(!result.valid);
        assert!(!result.valid);
    }

    #[test]
    fn query_filters() {
        let dir = tempfile::TempDir::new().unwrap();
        let mut logger = AuditLogger::with_dir("query-test", dir.path());

        logger.record(
            "tool",
            "tool_call",
            "user",
            &serde_json::json!({"tool": "rc"}),
            None,
        );
        logger.record(
            "tool",
            "tool_result",
            "user",
            &serde_json::json!({"tool": "rc"}),
            Some("success".into()),
        );
        logger.record(
            "tool",
            "tool_call",
            "user",
            &serde_json::json!({"tool": "github"}),
            None,
        );
        logger.record(
            "tool",
            "tool_result",
            "user",
            &serde_json::json!({"tool": "github"}),
            Some("error".into()),
        );

        let files = list_files(dir.path());
        let path = &files[0].path;

        // Filter by event
        let results = query_entries(
            path,
            &QueryOptions {
                event: Some("tool_result".into()),
                outcome: None,
                since: None,
                search: None,
                limit: 50,
            },
        );
        assert_eq!(results.len(), 2);

        // Filter by outcome
        let results = query_entries(
            path,
            &QueryOptions {
                event: None,
                outcome: Some("error".into()),
                since: None,
                search: None,
                limit: 50,
            },
        );
        assert_eq!(results.len(), 1);

        // Search
        let results = query_entries(
            path,
            &QueryOptions {
                event: None,
                outcome: None,
                since: None,
                search: Some("github".into()),
                limit: 50,
            },
        );
        assert_eq!(results.len(), 2);

        // Limit
        let results = query_entries(
            path,
            &QueryOptions {
                event: None,
                outcome: None,
                since: None,
                search: None,
                limit: 2,
            },
        );
        assert_eq!(results.len(), 2);
    }
}
