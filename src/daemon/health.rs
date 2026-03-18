use std::{sync::Arc, time::Duration};

use tokio::{sync::RwLock, task::JoinSet};
use tokio_util::sync::CancellationToken;

use crate::{
    config::{GRACE_PERIOD_CYCLES, HEALTH_CHECK_TIMEOUT_SECS, HEALTH_INTERVAL_SECS},
    daemon::state::{DaemonState, Status, ToolStatus},
    health,
    manifest::ToolType,
    mcp,
    registry::{Registry, Tool},
};

/// Result of a single tool health probe.
#[derive(Debug)]
pub struct ProbeResult {
    pub tool_name: String,
    pub healthy: bool,
    pub error: Option<String>,
}

/// Run a single health probe for a tool.
pub async fn probe_tool(tool: &Tool, daemon_state: Option<&DaemonState>) -> ProbeResult {
    let name = tool.manifest.name.clone();

    // Tools with a health command: use existing `health::check_one`
    if tool.manifest.health.is_some() {
        let result = health::check_one(tool).await;
        return ProbeResult {
            tool_name: name,
            healthy: result.unwrap_or(false),
            error: if result == Some(false) {
                Some("health command failed".to_string())
            } else {
                None
            },
        };
    }

    // MCP tools without health command: `tools/list` fallback
    if tool.manifest.tool_type == ToolType::Mcp {
        let result = tokio::time::timeout(
            Duration::from_secs(HEALTH_CHECK_TIMEOUT_SECS),
            probe_mcp_tools_list(tool, daemon_state),
        )
        .await;

        return match result {
            Ok(Ok(())) => ProbeResult {
                tool_name: name,
                healthy: true,
                error: None,
            },
            Ok(Err(e)) => ProbeResult {
                tool_name: name,
                healthy: false,
                error: Some(e.to_string()),
            },
            Err(_) => ProbeResult {
                tool_name: name,
                healthy: false,
                error: Some("probe timed out".to_string()),
            },
        };
    }

    // Native tools without health command: assumed healthy
    ProbeResult {
        tool_name: name,
        healthy: true,
        error: None,
    }
}

/// Probe an MCP tool by spawning a session and calling `tools/list`.
async fn probe_mcp_tools_list(
    tool: &Tool,
    _daemon_state: Option<&DaemonState>,
) -> Result<(), crate::error::ToolshedError> {
    let mcp_cfg = tool.manifest.mcp.as_ref().ok_or_else(|| {
        crate::error::ToolshedError::MissingMcpConfig {
            tool: tool.manifest.name.clone(),
        }
    })?;
    match mcp_cfg.transport {
        crate::manifest::McpTransport::Stdio => {
            let _tools = mcp::stdio::list_tools(tool).await?;
        }
        crate::manifest::McpTransport::Http => {
            let _tools = mcp::http::list_tools(tool).await?;
        }
    }
    Ok(())
}

/// Run all health probes in parallel.
pub async fn probe_all(
    registry: &Registry,
    daemon_state: &Arc<RwLock<DaemonState>>,
) -> Vec<ProbeResult> {
    let state_arc = daemon_state.clone();
    let mut join_set = JoinSet::new();

    for tool in registry.tools.values() {
        let tool_clone = tool.clone();
        let st = state_arc.clone();
        join_set.spawn(async move {
            let state = st.read().await;
            probe_tool(&tool_clone, Some(&state)).await
        });
    }

    let mut results = Vec::new();
    while let Some(Ok(result)) = join_set.join_next().await {
        results.push(result);
    }
    results
}

/// Main health loop. Runs until cancelled.
pub async fn run_health_loop(
    registry: Arc<Registry>,
    daemon_state: Arc<RwLock<DaemonState>>,
    cancel: CancellationToken,
    recovery_tx: tokio::sync::mpsc::Sender<String>,
) {
    loop {
        tokio::select! {
            () = cancel.cancelled() => {
                return;
            }
            () = tokio::time::sleep(Duration::from_secs(HEALTH_INTERVAL_SECS)) => {}
        }

        let skip_list: Vec<String> = {
            let state = daemon_state.read().await;
            state
                .tool_status
                .iter()
                .filter(|(_, ts)| ts.in_grace_period())
                .map(|(name, _)| name.clone())
                .collect()
        };

        let results = probe_all(&registry, &daemon_state).await;

        let recovery_targets: Vec<String>;
        {
            let mut state = daemon_state.write().await;
            let mut targets = Vec::new();
            for result in results {
                if skip_list.contains(&result.tool_name) {
                    continue;
                }

                let ts = state
                    .tool_status
                    .entry(result.tool_name.clone())
                    .or_insert_with(ToolStatus::new_up);

                if result.healthy {
                    if ts.status == Status::Up {
                        // Already up, just update check time
                    } else {
                        ts.status = Status::Up;
                        ts.consecutive_failures = 0;
                        ts.last_error = None;
                        ts.recovering_attempt = None;
                    }
                    ts.last_check = std::time::Instant::now();
                } else {
                    let was_up = ts.status == Status::Up;
                    ts.mark_down(&result.error.unwrap_or_default());
                    if was_up || ts.status == Status::Down {
                        targets.push(result.tool_name.clone());
                    }
                }
            }

            // Clear expired grace periods
            for ts in state.tool_status.values_mut() {
                if let Some(g) = ts.grace_until {
                    if ts.status == Status::Up && std::time::Instant::now() >= g {
                        ts.grace_until = None;
                    }
                }
            }
            drop(state);
            recovery_targets = targets;
        }
        for target in &recovery_targets {
            let _ = recovery_tx.send(target.clone()).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_result_construction() {
        let pr = ProbeResult {
            tool_name: "test-tool".to_string(),
            healthy: true,
            error: None,
        };
        assert!(pr.healthy);
        assert!(pr.error.is_none());
    }
}
