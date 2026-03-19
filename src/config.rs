use std::path::PathBuf;

pub const DEFAULT_MAX_OUTPUT: usize = 4096;
pub const INTROSPECT_CACHE_TTL_SECS: u64 = 3600;
pub const HEALTH_CHECK_TIMEOUT_SECS: u64 = 15;
pub const DEFAULT_TOOL_TIMEOUT_SECS: u64 = 120;

// Daemon constants
pub const HEALTH_INTERVAL_SECS: u64 = 30;
pub const MAX_RECOVERY_ATTEMPTS: u8 = 3;
pub const RECOVERY_BACKOFF: [u64; 3] = [0, 2, 12];
pub const AUTH_REFRESH_RATIO: f64 = 0.8;
pub const AUTH_DEFAULT_TTL_SECS: u64 = 3600;
pub const AUTH_RETRY_ATTEMPTS: u8 = 3;
pub const GRACE_PERIOD_CYCLES: u64 = 1;
/// Cold-tier tools are probed every Nth cycle (30s x 10 = 5 min).
pub const COLD_TIER_CYCLE_MULTIPLIER: u64 = 10;

pub fn toolshed_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("TOOLSHED_DIR") {
        return PathBuf::from(dir);
    }
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join(".toolshed")
}

pub fn tools_dir() -> PathBuf {
    toolshed_dir().join("tools")
}

pub fn cache_dir() -> PathBuf {
    toolshed_dir().join("cache")
}

pub fn audit_dir() -> PathBuf {
    toolshed_dir().join("audit")
}

pub fn skills_dir() -> PathBuf {
    toolshed_dir().join("skills")
}

pub fn agents_dir() -> PathBuf {
    toolshed_dir().join("agents")
}

pub fn rules_dir() -> PathBuf {
    toolshed_dir().join("rules")
}

pub fn workflows_dir() -> PathBuf {
    toolshed_dir().join("workflows")
}

pub const DEFAULT_WORKFLOW_TIMEOUT_SECS: u64 = 600;
