use std::{sync::Arc, time::Duration};

use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

use crate::{
    config::{
        AUTH_REFRESH_RATIO, GRACE_PERIOD_CYCLES, HEALTH_INTERVAL_SECS, MAX_RECOVERY_ATTEMPTS,
        RECOVERY_BACKOFF,
    },
    daemon::{
        auth,
        health::probe_tool,
        state::{DaemonState, Status},
    },
    error::ToolshedError,
    registry::Registry,
};

/// Run the recovery pipeline for a single tool.
/// Returns `Ok(())` if recovery succeeded, `Err` if exhausted (caller
/// should crash).
pub async fn recover_tool(
    tool_name: &str,
    registry: &Registry,
    daemon_state: Arc<RwLock<DaemonState>>,
    vault_addr: Option<&str>,
    vault_token: Option<&str>,
    http_client: &reqwest::Client,
) -> Result<(), ToolshedError> {
    let tool = registry
        .tools
        .get(tool_name)
        .ok_or_else(|| ToolshedError::ToolNotFound {
            name: tool_name.to_string(),
        })?;

    let mut attempt_errors: Vec<String> = Vec::new();

    for attempt in 0..MAX_RECOVERY_ATTEMPTS {
        let attempt_num = attempt + 1;
        let delay = RECOVERY_BACKOFF[usize::from(attempt)];

        // Mark recovering
        {
            let mut state = daemon_state.write().await;
            if let Some(ts) = state.tool_status.get_mut(tool_name) {
                ts.mark_recovering(attempt_num);
            }
        }

        if delay > 0 {
            tokio::time::sleep(Duration::from_secs(delay)).await;
        }

        // Attempt 2+: refresh secrets
        if attempt >= 1 {
            if let (Some(addr), Some(token)) = (vault_addr, vault_token) {
                refresh_tool_secrets(tool_name, &daemon_state, http_client, addr, token).await;
            }
        }

        // Attempt 3: clear introspection cache
        if attempt == 2 {
            clear_introspection_cache(tool_name);
        }

        // Probe (pass `DaemonState` so refreshed secrets are used)
        let state_snapshot = daemon_state.read().await;
        let probe = probe_tool(tool, Some(&state_snapshot)).await;
        drop(state_snapshot);

        if probe.healthy {
            let grace = Duration::from_secs(HEALTH_INTERVAL_SECS * GRACE_PERIOD_CYCLES);
            {
                let mut state = daemon_state.write().await;
                if let Some(ts) = state.tool_status.get_mut(tool_name) {
                    ts.mark_up_after_recovery(grace);
                }
            }
            return Ok(());
        }

        let err_msg = probe.error.unwrap_or_else(|| "unknown error".to_string());
        attempt_errors.push(format!("attempt {attempt_num}: {err_msg}"));
    }

    Err(ToolshedError::RecoveryExhausted {
        tool: tool_name.to_string(),
        reason: attempt_errors.join("; "),
    })
}

/// Refresh secrets for a specific tool from Vault.
async fn refresh_tool_secrets(
    _tool_name: &str,
    daemon_state: &Arc<RwLock<DaemonState>>,
    client: &reqwest::Client,
    vault_addr: &str,
    vault_token: &str,
) {
    let entries_to_refresh: Vec<(String, String, String)> = {
        let state = daemon_state.read().await;
        state
            .secrets
            .values()
            .map(|e| (e.env_var.clone(), e.path.clone(), e.key.clone()))
            .collect()
    };

    for (env_var, path, key) in entries_to_refresh {
        if let Ok((value, ttl)) =
            auth::vault_get(client, vault_addr, vault_token, &path, &key).await
        {
            let now = std::time::Instant::now();
            let jittered = auth::jittered_refresh_secs(ttl.as_secs(), AUTH_REFRESH_RATIO);
            let mut state = daemon_state.write().await;
            if let Some(entry) = state.secrets.get_mut(&env_var) {
                entry.value = secrecy::SecretString::from(value);
                entry.lease_ttl = ttl;
                entry.refreshed_at = now;
                entry.next_refresh_at = now + jittered;
            }
        }
    }
}

/// Clear the MCP introspection cache file for a tool.
fn clear_introspection_cache(tool_name: &str) {
    let cache_path = crate::config::cache_dir().join(format!("{tool_name}.tools.json"));
    if cache_path.exists() {
        let _ = std::fs::remove_file(&cache_path);
    }
}

/// Listen for recovery requests and spawn recovery tasks.
pub async fn run_recovery_listener(
    mut rx: tokio::sync::mpsc::Receiver<String>,
    registry: Arc<Registry>,
    daemon_state: Arc<RwLock<DaemonState>>,
    cancel: CancellationToken,
    vault_addr: Option<String>,
    vault_token: Option<String>,
) {
    let http_client = reqwest::Client::new();

    loop {
        tokio::select! {
            () = cancel.cancelled() => {
                return;
            }
            tool_name = rx.recv() => {
                let Some(tool_name) = tool_name else {
                    return;
                };

                // Skip if already recovering
                {
                    let state = daemon_state.read().await;
                    if let Some(ts) = state.tool_status.get(&tool_name) {
                        if ts.status == Status::Recovering {
                            continue;
                        }
                    }
                }

                let registry = registry.clone();
                let daemon_state = daemon_state.clone();
                let vault_addr = vault_addr.clone();
                let vault_token = vault_token.clone();
                let http_client = http_client.clone();

                tokio::spawn(async move {
                    let result = recover_tool(
                        &tool_name,
                        &registry,
                        daemon_state,
                        vault_addr.as_deref(),
                        vault_token.as_deref(),
                        &http_client,
                    )
                    .await;

                    if let Err(_e) = result {
                        std::process::exit(1);
                    }
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::config::RECOVERY_BACKOFF;

    #[test]
    fn recovery_backoff_values() {
        assert_eq!(RECOVERY_BACKOFF[0], 0);
        assert_eq!(RECOVERY_BACKOFF[1], 2);
        assert_eq!(RECOVERY_BACKOFF[2], 12);
    }

    #[test]
    fn clear_nonexistent_cache_is_noop() {
        super::clear_introspection_cache("nonexistent-tool-xyzzy");
    }
}
