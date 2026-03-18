use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use secrecy::SecretString;
use serde::Deserialize;

use crate::{
    config::{AUTH_DEFAULT_TTL_SECS, AUTH_REFRESH_RATIO},
    daemon::state::SecretEntry,
    error::ToolshedError,
};

/// Vault KV v2 response shape.
#[derive(Debug, Deserialize)]
struct VaultResponse {
    data: VaultDataWrapper,
    lease_duration: u64,
}

#[derive(Debug, Deserialize)]
struct VaultDataWrapper {
    data: HashMap<String, String>,
}

/// Vault `AppRole` login response.
#[derive(Debug, Deserialize)]
struct VaultAuthResponse {
    auth: VaultAuth,
}

#[derive(Debug, Deserialize)]
struct VaultAuth {
    client_token: String,
    lease_duration: u64,
}

/// Maps a Vault path/key to an env var name.
#[derive(Debug, Clone)]
pub struct SecretDef {
    pub path: String,
    pub key: String,
    pub env_var: String,
}

/// Parse the `vault-env.sh` script to extract secret definitions.
/// Each line matching `v=$(vault_get <path> <key>); ... export
/// <ENV_VAR>="$v"` becomes a `SecretDef`.
pub fn parse_vault_env_script(script: &str) -> Vec<SecretDef> {
    let mut defs = Vec::new();
    for line in script.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("v=$(vault_get ") {
            if let Some(paren_pos) = rest.find(')') {
                let args = &rest[..paren_pos];
                let parts: Vec<&str> = args.split_whitespace().collect();
                if parts.len() >= 2 {
                    let path = parts[0];
                    let key = parts[1];
                    if let Some(export_pos) = line.find("export ") {
                        let after_export = &line[export_pos + 7..];
                        if let Some(eq_pos) = after_export.find('=') {
                            let env_var = &after_export[..eq_pos];
                            defs.push(SecretDef {
                                path: path.to_string(),
                                key: key.to_string(),
                                env_var: env_var.to_string(),
                            });
                        }
                    }
                }
            }
        }
    }
    defs
}

/// Fetch a single secret from Vault KV v2.
pub async fn vault_get(
    client: &reqwest::Client,
    vault_addr: &str,
    vault_token: &str,
    path: &str,
    key: &str,
) -> Result<(String, Duration), ToolshedError> {
    let url = format!("{vault_addr}/v1/secret/data/{path}");
    let resp = client
        .get(&url)
        .header("X-Vault-Token", vault_token)
        .send()
        .await
        .map_err(|e| ToolshedError::VaultError {
            reason: format!("request to {url} failed: {e}"),
        })?;

    let status = resp.status();
    if status == reqwest::StatusCode::FORBIDDEN {
        return Err(ToolshedError::VaultAuthFailed {
            reason: format!("403 permission denied for {path}"),
        });
    }
    if status == reqwest::StatusCode::NOT_FOUND {
        return Err(ToolshedError::VaultError {
            reason: format!("404 secret not found: {path}"),
        });
    }
    if status == reqwest::StatusCode::SERVICE_UNAVAILABLE {
        return Err(ToolshedError::VaultError {
            reason: "503 vault sealed or unavailable".to_string(),
        });
    }
    if !status.is_success() {
        return Err(ToolshedError::VaultError {
            reason: format!("unexpected status {status} for {path}"),
        });
    }

    let body: VaultResponse = resp.json().await.map_err(|e| ToolshedError::VaultError {
        reason: format!("failed to parse Vault response for {path}: {e}"),
    })?;

    let value = body
        .data
        .data
        .get(key)
        .ok_or_else(|| ToolshedError::VaultError {
            reason: format!("key '{key}' not found in secret '{path}'"),
        })?
        .clone();

    let ttl = if body.lease_duration == 0 {
        Duration::from_secs(AUTH_DEFAULT_TTL_SECS)
    } else {
        Duration::from_secs(body.lease_duration)
    };

    Ok((value, ttl))
}

/// Attempt `AppRole` login to get a fresh Vault token.
pub async fn vault_approle_login(
    client: &reqwest::Client,
    vault_addr: &str,
    role_id: &str,
    secret_id: &str,
) -> Result<(String, Duration), ToolshedError> {
    let url = format!("{vault_addr}/v1/auth/approle/login");
    let body = serde_json::json!({
        "role_id": role_id,
        "secret_id": secret_id,
    });

    let resp =
        client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| ToolshedError::VaultAuthFailed {
                reason: format!("AppRole login request failed: {e}"),
            })?;

    if !resp.status().is_success() {
        return Err(ToolshedError::VaultAuthFailed {
            reason: format!("AppRole login returned {}", resp.status()),
        });
    }

    let auth_resp: VaultAuthResponse =
        resp.json()
            .await
            .map_err(|e| ToolshedError::VaultAuthFailed {
                reason: format!("failed to parse AppRole response: {e}"),
            })?;

    let ttl = Duration::from_secs(auth_resp.auth.lease_duration);
    Ok((auth_resp.auth.client_token, ttl))
}

/// Resolve all secrets from Vault and build `SecretEntry` map.
pub async fn resolve_all_secrets(
    client: &reqwest::Client,
    vault_addr: &str,
    vault_token: &str,
    defs: &[SecretDef],
) -> Result<HashMap<String, SecretEntry>, ToolshedError> {
    let mut secrets = HashMap::new();
    for def in defs {
        let (value, ttl) = vault_get(client, vault_addr, vault_token, &def.path, &def.key).await?;
        let now = Instant::now();
        let jittered = jittered_refresh_secs(ttl.as_secs(), AUTH_REFRESH_RATIO);
        secrets.insert(
            def.env_var.clone(),
            SecretEntry {
                path: def.path.clone(),
                key: def.key.clone(),
                env_var: def.env_var.clone(),
                value: SecretString::from(value),
                lease_ttl: ttl,
                refreshed_at: now,
                next_refresh_at: now + jittered,
            },
        );
    }
    Ok(secrets)
}

/// Compute jittered refresh duration: `ratio * ttl +/- 10%`.
/// Called once per secret resolve/refresh; result is stored, not
/// recomputed.
pub fn jittered_refresh_secs(ttl_secs: u64, ratio: f64) -> Duration {
    #[allow(clippy::cast_precision_loss)]
    let base = ttl_secs as f64 * ratio;
    let jitter_range = base * 0.1;
    let jitter = rand::random::<f64>().mul_add(2.0 * jitter_range, -jitter_range);
    let secs = (base + jitter).max(1.0);
    Duration::from_secs_f64(secs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_vault_env_script_extracts_defs() {
        let script = r#"v=$(vault_get gitlab token); [ -n "$v" ] && export GITLAB_TOKEN="$v"
v=$(vault_get sourcegraph token); [ -n "$v" ] && export SRC_ACCESS_TOKEN="$v"
v=$(vault_get huly/credentials HULY_EMAIL); [ -n "$v" ] && export HULY_EMAIL="$v"
"#;
        let defs = parse_vault_env_script(script);
        assert_eq!(defs.len(), 3);

        assert_eq!(defs[0].path, "gitlab");
        assert_eq!(defs[0].key, "token");
        assert_eq!(defs[0].env_var, "GITLAB_TOKEN");

        assert_eq!(defs[2].path, "huly/credentials");
        assert_eq!(defs[2].key, "HULY_EMAIL");
        assert_eq!(defs[2].env_var, "HULY_EMAIL");
    }

    #[test]
    fn jittered_refresh_within_bounds() {
        for _ in 0..100 {
            let d = jittered_refresh_secs(3600, 0.8);
            let secs = d.as_secs_f64();
            assert!((2590.0..=3170.0).contains(&secs), "got {secs}");
        }
    }

    #[test]
    fn jittered_refresh_zero_ttl() {
        let d = jittered_refresh_secs(0, 0.8);
        assert!(d.as_secs_f64() >= 1.0);
    }
}
