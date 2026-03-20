use std::{sync::Arc, time::Duration};

use tokio::{sync::RwLock, task::JoinSet};
use tokio_util::sync::CancellationToken;

use crate::{
    config::{COLD_TIER_CYCLE_MULTIPLIER, GRACE_PERIOD_CYCLES, HEALTH_INTERVAL_SECS},
    daemon::state::{DaemonState, Status, ToolStatus},
    health,
    manifest::HealthTier,
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
pub async fn probe_tool(tool: &Tool, _daemon_state: Option<&DaemonState>) -> ProbeResult {
    let name = tool.manifest.name.clone();

    // All tools must have a health command (enforced at registry load).
    let Some(ref _health_cmd) = tool.manifest.health else {
        return ProbeResult {
            tool_name: name,
            healthy: false,
            error: Some("no health command configured".to_string()),
        };
    };

    match health::check_one(tool).await {
        Some(Ok(())) => ProbeResult {
            tool_name: name,
            healthy: true,
            error: None,
        },
        Some(Err(detail)) => ProbeResult {
            tool_name: name,
            healthy: false,
            error: Some(detail),
        },
        None => ProbeResult {
            tool_name: name,
            healthy: false,
            error: Some("no health command configured".to_string()),
        },
    }
}

/// Determine whether a tool should be probed this cycle based on its tier.
const fn should_probe_tier(tier: Option<HealthTier>, cycle: u64) -> bool {
    match tier {
        Some(HealthTier::Cold) => cycle.is_multiple_of(COLD_TIER_CYCLE_MULTIPLIER),
        Some(HealthTier::Hot) | None => true,
    }
}

/// Run all health probes in parallel.
pub async fn probe_all(
    registry: &Registry,
    daemon_state: &Arc<RwLock<DaemonState>>,
    cycle: u64,
) -> Vec<ProbeResult> {
    let state_arc = daemon_state.clone();
    let mut join_set = JoinSet::new();

    for tool in registry.tools.values() {
        if !should_probe_tier(tool.manifest.tier, cycle) {
            continue;
        }
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
    let mut cycle: u64 = 0;
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
                .filter(|(_, ts)| ts.in_grace_period() || ts.status == Status::FailedToLoad)
                .map(|(name, _)| name.clone())
                .collect()
        };

        let results = probe_all(&registry, &daemon_state, cycle).await;

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
        cycle = cycle.wrapping_add(1);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
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

    #[tokio::test]
    async fn probe_tool_without_health_is_unhealthy() {
        use crate::manifest::{ToolManifest, ToolType};
        use crate::registry::Tool;
        use std::collections::BTreeMap;

        let tool = Tool {
            dir: std::path::PathBuf::from("/tmp/fake"),
            manifest: ToolManifest {
                name: "no-health-tool".to_string(),
                description: "Test".to_string(),
                category: "test".to_string(),
                tool_type: ToolType::Mcp,
                max_output: 4096,
                health: None,
                tier: None,
                commands: BTreeMap::new(),
                mcp: None,
            },
            run_path: None,
        };
        let result = probe_tool(&tool, None).await;
        assert!(!result.healthy);
        assert!(result.error.is_some());
    }

    #[test]
    fn should_probe_hot_every_cycle() {
        assert!(should_probe_tier(Some(HealthTier::Hot), 0));
        assert!(should_probe_tier(Some(HealthTier::Hot), 1));
        assert!(should_probe_tier(Some(HealthTier::Hot), 9));
        assert!(should_probe_tier(Some(HealthTier::Hot), 10));
    }

    #[test]
    fn should_probe_cold_only_on_multiplier() {
        assert!(should_probe_tier(Some(HealthTier::Cold), 0));
        assert!(!should_probe_tier(Some(HealthTier::Cold), 1));
        assert!(!should_probe_tier(Some(HealthTier::Cold), 9));
        assert!(should_probe_tier(Some(HealthTier::Cold), 10));
        assert!(!should_probe_tier(Some(HealthTier::Cold), 11));
        assert!(should_probe_tier(Some(HealthTier::Cold), 20));
    }

    #[test]
    fn should_probe_no_tier_every_cycle() {
        assert!(should_probe_tier(None, 0));
        assert!(should_probe_tier(None, 5));
    }
}
