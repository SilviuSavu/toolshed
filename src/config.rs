use std::path::PathBuf;

pub const DEFAULT_MAX_OUTPUT: usize = 4096;
pub const DEFAULT_IDLE_TIMEOUT: u64 = 300;
pub const HEALTH_CACHE_TTL_SECS: u64 = 30;
pub const INTROSPECT_CACHE_TTL_SECS: u64 = 3600;
pub const HEALTH_CHECK_TIMEOUT_SECS: u64 = 5;
pub const DEFAULT_TOOL_TIMEOUT_SECS: u64 = 120;

pub fn toolshed_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("TOOLSHED_DIR") {
        return PathBuf::from(dir);
    }
    dirs::home_dir()
        .expect("cannot determine home directory")
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
