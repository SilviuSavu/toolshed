use crate::config::HEALTH_CHECK_TIMEOUT_SECS;
use crate::registry::{Registry, Tool};
use std::collections::HashMap;
use std::time::Duration;
use tokio::process::Command;

/// Check health for a single tool. Returns Some(true/false) if health is configured, None otherwise.
pub async fn check_one(tool: &Tool) -> Option<bool> {
    let health_cmd = tool.manifest.health.as_ref()?;
    Some(run_health_check(health_cmd).await)
}

/// Check health for all tools in the registry. Returns a map of tool_name -> Option<bool>.
pub async fn check_all(registry: &Registry) -> HashMap<String, Option<bool>> {
    let mut handles = Vec::new();

    for (name, tool) in &registry.tools {
        let name = name.clone();
        let health_cmd = tool.manifest.health.clone();
        handles.push(tokio::spawn(async move {
            let result = match &health_cmd {
                Some(cmd) => Some(run_health_check(cmd).await),
                None => None,
            };
            (name, result)
        }));
    }

    let mut results = HashMap::new();
    for handle in handles {
        if let Ok((name, result)) = handle.await {
            results.insert(name, result);
        }
    }

    results
}

async fn run_health_check(cmd: &str) -> bool {
    let result = tokio::time::timeout(
        Duration::from_secs(HEALTH_CHECK_TIMEOUT_SECS),
        Command::new("sh").arg("-c").arg(cmd).output(),
    )
    .await;

    match result {
        Ok(Ok(output)) => output.status.success(),
        _ => false,
    }
}
