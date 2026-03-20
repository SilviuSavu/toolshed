pub mod auth;
pub mod health;
pub mod recovery;
pub mod state;

use std::{sync::Arc, time::Duration};

use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

use self::{auth::SecretDef, state::DaemonState};
use crate::{
    config::{AUTH_REFRESH_RATIO, AUTH_RETRY_ATTEMPTS},
    registry::Registry,
};

/// Spawn the daemon as a set of background tasks.
/// Returns the `CancellationToken` for shutdown and the shared
/// `DaemonState`.
pub async fn spawn_daemon(
    registry: Arc<Registry>,
    secret_defs: Vec<SecretDef>,
    vault_addr: Option<String>,
    vault_token: Option<String>,
) -> (CancellationToken, Arc<RwLock<DaemonState>>) {
    let cancel = CancellationToken::new();
    let daemon_state = Arc::new(RwLock::new(DaemonState::new()));

    // Initialize tool statuses
    {
        let mut st = daemon_state.write().await;
        for name in registry.tools.keys() {
            st.tool_status
                .insert(name.clone(), state::ToolStatus::new_up());
        }
    }

    // Resolve initial secrets from Vault
    if let (Some(ref addr), Some(ref token)) = (&vault_addr, &vault_token) {
        if !secret_defs.is_empty() {
            let client = reqwest::Client::new();
            match auth::resolve_all_secrets(&client, addr, token, &secret_defs).await {
                Ok(secrets) => {
                    let mut st = daemon_state.write().await;
                    st.secrets = secrets;
                }
                #[allow(clippy::print_stderr)]
                Err(e) => {
                    eprintln!("ERROR: initial Vault secret resolution failed: {e}");
                    eprintln!("  Tools requiring Vault credentials will not function until secrets are resolved.");
                }
            }
        }
    }

    // Recovery channel
    let (recovery_tx, recovery_rx) = tokio::sync::mpsc::channel::<String>(32);

    // Spawn health loop
    let h_reg = registry.clone();
    let h_state = daemon_state.clone();
    let h_cancel = cancel.clone();
    tokio::spawn(async move {
        health::run_health_loop(h_reg, h_state, h_cancel, recovery_tx).await;
    });

    // Spawn recovery listener
    let r_reg = registry.clone();
    let r_state = daemon_state.clone();
    let r_cancel = cancel.clone();
    let r_vault_addr = vault_addr.clone();
    let r_vault_token = vault_token.clone();
    tokio::spawn(async move {
        recovery::run_recovery_listener(
            recovery_rx,
            r_reg,
            r_state,
            r_cancel,
            r_vault_addr,
            r_vault_token,
        )
        .await;
    });

    // Spawn auth refresh loop
    if let (Some(a_addr), Some(a_token)) = (vault_addr, vault_token) {
        if !secret_defs.is_empty() {
            let a_state = daemon_state.clone();
            let a_cancel = cancel.clone();
            tokio::spawn(async move {
                run_auth_refresh_loop(a_state, a_cancel, &a_addr, &a_token).await;
            });
        }
    }

    (cancel, daemon_state)
}

/// Auth refresh loop: sleeps until the soonest secret needs refresh.
#[allow(clippy::print_stderr)]
async fn run_auth_refresh_loop(
    daemon_state: Arc<RwLock<DaemonState>>,
    cancel: CancellationToken,
    vault_addr: &str,
    vault_token: &str,
) {
    let client = reqwest::Client::new();

    {
        let mut st = daemon_state.write().await;
        for entry in st.secrets.values_mut() {
            let jittered =
                auth::jittered_refresh_secs(entry.lease_ttl.as_secs(), AUTH_REFRESH_RATIO);
            entry.next_refresh_at = entry.refreshed_at + jittered;
        }
    }

    loop {
        let sleep_duration = {
            let st = daemon_state.read().await;
            if st.secrets.is_empty() {
                Duration::from_secs(60)
            } else {
                let now = std::time::Instant::now();
                st.secrets
                    .values()
                    .map(|e| e.next_refresh_at.saturating_duration_since(now))
                    .min()
                    .unwrap_or(Duration::from_secs(60))
            }
        };

        tokio::select! {
            () = cancel.cancelled() => { return; }
            () = tokio::time::sleep(sleep_duration) => {}
        }

        let to_refresh: Vec<(String, String, String)> = {
            let st = daemon_state.read().await;
            let now = std::time::Instant::now();
            st.secrets
                .values()
                .filter(|e| now >= e.next_refresh_at)
                .map(|e| (e.env_var.clone(), e.path.clone(), e.key.clone()))
                .collect()
        };

        for (env_var, path, key) in to_refresh {
            let mut last_err = None;
            for retry in 0..AUTH_RETRY_ATTEMPTS {
                match auth::vault_get(&client, vault_addr, vault_token, &path, &key).await {
                    Ok((value, ttl)) => {
                        let now = std::time::Instant::now();
                        let jittered =
                            auth::jittered_refresh_secs(ttl.as_secs(), AUTH_REFRESH_RATIO);
                        {
                            let mut st = daemon_state.write().await;
                            if let Some(entry) = st.secrets.get_mut(&env_var) {
                                entry.value = secrecy::SecretString::from(value);
                                entry.lease_ttl = ttl;
                                entry.refreshed_at = now;
                                entry.next_refresh_at = now + jittered;
                            }
                        }
                        last_err = None;
                        break;
                    }
                    Err(e) => {
                        last_err = Some(e);
                        if retry < AUTH_RETRY_ATTEMPTS - 1 {
                            let backoff = Duration::from_secs(u64::from(retry + 1) * 2);
                            tokio::time::sleep(backoff).await;
                        }
                    }
                }
            }
            if let Some(err) = last_err {
                eprintln!(
                    "ERROR: failed to refresh secret '{env_var}' after {AUTH_RETRY_ATTEMPTS} attempts: {err}"
                );
            }
        }
    }
}
