use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use secrecy::SecretString;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Up,
    Down,
    Recovering,
    /// Tool failed during introspection/loading and cannot be recovered
    /// by the daemon. Requires a code or configuration fix and a restart.
    FailedToLoad,
}

#[derive(Debug)]
pub struct ToolStatus {
    pub status: Status,
    pub last_check: Instant,
    pub consecutive_failures: u16,
    pub recoveries: u32,
    pub last_error: Option<String>,
    pub recovering_attempt: Option<u8>,
    pub grace_until: Option<Instant>,
}

impl ToolStatus {
    pub fn new_up() -> Self {
        Self {
            status: Status::Up,
            last_check: Instant::now(),
            consecutive_failures: 0,
            recoveries: 0,
            last_error: None,
            recovering_attempt: None,
            grace_until: None,
        }
    }

    pub fn mark_failed_to_load(&mut self, error: &str) {
        self.status = Status::FailedToLoad;
        self.last_error = Some(truncate_error(error));
        self.last_check = Instant::now();
    }

    pub fn mark_down(&mut self, error: &str) {
        self.status = Status::Down;
        self.consecutive_failures = self.consecutive_failures.saturating_add(1);
        self.last_error = Some(truncate_error(error));
        self.last_check = Instant::now();
    }

    pub fn mark_recovering(&mut self, attempt: u8) {
        self.status = Status::Recovering;
        self.recovering_attempt = Some(attempt);
        self.last_check = Instant::now();
    }

    pub fn mark_up_after_recovery(&mut self, grace: Duration) {
        self.status = Status::Up;
        self.consecutive_failures = 0;
        self.recoveries = self.recoveries.saturating_add(1);
        self.last_error = None;
        self.recovering_attempt = None;
        self.grace_until = Some(Instant::now() + grace);
        self.last_check = Instant::now();
    }

    pub fn in_grace_period(&self) -> bool {
        self.grace_until.is_some_and(|g| Instant::now() < g)
    }
}

fn truncate_error(err: &str) -> String {
    let chars: Vec<char> = err.chars().collect();
    if chars.len() <= 256 {
        err.to_string()
    } else {
        let mut s: String = chars[..256].iter().collect();
        s.push_str("...");
        s
    }
}

#[derive(Debug)]
pub struct SecretEntry {
    pub path: String,
    pub key: String,
    pub env_var: String,
    pub value: SecretString,
    pub lease_ttl: Duration,
    pub refreshed_at: Instant,
    /// Pre-computed jittered refresh time. Set once on resolve/refresh,
    /// not recomputed on each loop iteration (prevents drift from random
    /// jitter).
    pub next_refresh_at: Instant,
}

#[derive(Debug)]
pub struct DaemonState {
    pub tool_status: HashMap<String, ToolStatus>,
    pub secrets: HashMap<String, SecretEntry>,
    pub started_at: Instant,
}

impl DaemonState {
    pub fn new() -> Self {
        Self {
            tool_status: HashMap::new(),
            secrets: HashMap::new(),
            started_at: Instant::now(),
        }
    }

    /// Look up a secret value by env var name.
    pub fn get_secret(&self, env_var: &str) -> Option<&SecretEntry> {
        self.secrets.values().find(|s| s.env_var == env_var)
    }

    pub fn uptime_secs(&self) -> u64 {
        self.started_at.elapsed().as_secs()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_status_transitions() {
        let mut ts = ToolStatus::new_up();
        assert_eq!(ts.status, Status::Up);
        assert_eq!(ts.consecutive_failures, 0);
        assert_eq!(ts.recoveries, 0);

        ts.mark_down("connection refused");
        assert_eq!(ts.status, Status::Down);
        assert_eq!(ts.consecutive_failures, 1);
        assert_eq!(ts.last_error.as_deref(), Some("connection refused"));

        ts.mark_recovering(1);
        assert_eq!(ts.status, Status::Recovering);
        assert_eq!(ts.recovering_attempt, Some(1));

        ts.mark_up_after_recovery(Duration::from_secs(30));
        assert_eq!(ts.status, Status::Up);
        assert_eq!(ts.consecutive_failures, 0);
        assert_eq!(ts.recoveries, 1);
        assert!(ts.last_error.is_none());
        assert!(ts.recovering_attempt.is_none());
        assert!(ts.in_grace_period());
    }

    #[test]
    fn consecutive_failures_accumulate() {
        let mut ts = ToolStatus::new_up();
        ts.mark_down("err1");
        ts.mark_down("err2");
        ts.mark_down("err3");
        assert_eq!(ts.consecutive_failures, 3);

        ts.mark_up_after_recovery(Duration::ZERO);
        assert_eq!(ts.consecutive_failures, 0);
    }

    #[test]
    fn error_truncation() {
        let long = "x".repeat(500);
        let truncated = truncate_error(&long);
        assert_eq!(truncated.len(), 259); // 256 + "..."
        assert!(truncated.ends_with("..."));

        let short = "short error";
        assert_eq!(truncate_error(short), short);
    }

    #[test]
    fn daemon_state_get_secret() {
        let mut state = DaemonState::new();
        let now = Instant::now();
        state.secrets.insert(
            "gitlab".to_string(),
            SecretEntry {
                path: "vault/gitlab".to_string(),
                key: "token".to_string(),
                env_var: "GITLAB_TOKEN".to_string(),
                value: SecretString::from("abc123".to_string()),
                lease_ttl: Duration::from_secs(3600),
                refreshed_at: now,
                next_refresh_at: now + Duration::from_secs(2880),
            },
        );

        let found = state.get_secret("GITLAB_TOKEN");
        assert!(found.is_some());
        assert_eq!(found.map(|s| s.path.as_str()), Some("vault/gitlab"));

        let not_found = state.get_secret("NONEXISTENT");
        assert!(not_found.is_none());
    }

    #[test]
    fn daemon_state_uptime() {
        let state = DaemonState::new();
        assert!(state.uptime_secs() < 2);
    }

    #[test]
    fn multiple_recovery_cycles() {
        let mut ts = ToolStatus::new_up();

        ts.mark_down("err1");
        ts.mark_recovering(1);
        ts.mark_up_after_recovery(Duration::ZERO);
        assert_eq!(ts.recoveries, 1);

        ts.mark_down("err2");
        ts.mark_recovering(1);
        ts.mark_up_after_recovery(Duration::ZERO);
        assert_eq!(ts.recoveries, 2);
        assert_eq!(ts.consecutive_failures, 0);
    }
}
