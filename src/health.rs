use std::{collections::HashMap, path::Path, time::Duration};

use tokio::process::Command;

use crate::{
    config::HEALTH_CHECK_TIMEOUT_SECS,
    manifest::ToolType,
    registry::{Registry, Tool},
};

/// Outcome of a health check: `Ok(())` for healthy, `Err(detail)` for
/// unhealthy with a human-readable reason.
pub type HealthOutcome = Result<(), String>;

/// Check health for a single tool. Returns `None` if no health command
/// is configured.
pub async fn check_one(tool: &Tool) -> Option<HealthOutcome> {
    let health_cmd = tool.manifest.health.as_ref()?;
    let resolved = resolve_health_cmd(health_cmd, tool);
    Some(run_health_check(&resolved, &tool.dir).await)
}

/// Check health for all tools in the registry. Returns a map of
/// `tool_name` -> `Option<HealthOutcome>`.
pub async fn check_all(registry: &Registry) -> HashMap<String, Option<HealthOutcome>> {
    let mut handles = Vec::new();

    for (name, tool) in &registry.tools {
        let name = name.clone();
        let health_cmd = tool.manifest.health.clone();
        let resolved = health_cmd.as_ref().map(|cmd| resolve_health_cmd(cmd, tool));
        let tool_dir = tool.dir.clone();
        handles.push(tokio::spawn(async move {
            let result = match &resolved {
                Some(cmd) => Some(run_health_check(cmd, &tool_dir).await),
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

/// For native tools whose health command invokes `<tool-name> <command>` where
/// `<command>` is a defined manifest command, replace the tool name with the
/// `run` script path so it resolves without being on PATH.
///
/// Health commands that call a different binary (e.g. `jj version`,
/// `agent-browser --version`) are left unchanged.
fn resolve_health_cmd(cmd: &str, tool: &Tool) -> String {
    if tool.manifest.tool_type != ToolType::Native {
        return cmd.to_string();
    }

    let Some(run_path) = &tool.run_path else {
        return cmd.to_string();
    };

    let tool_name = &tool.manifest.name;

    // Only resolve if the command is exactly the tool name (no args)
    // and the manifest defines at least one command (so `run` accepts bare
    // invocation).
    if cmd == tool_name && !tool.manifest.commands.is_empty() {
        return run_path.display().to_string();
    }

    // Only resolve `<tool-name> <subcommand> ...` if <subcommand> is a
    // defined command in the manifest — otherwise the health check calls
    // an external binary that happens to share the tool name prefix.
    if let Some(rest) = cmd.strip_prefix(tool_name) {
        if let Some(after_space) = rest.strip_prefix(' ') {
            let subcmd = after_space.split_whitespace().next().unwrap_or("");
            if tool.manifest.commands.contains_key(subcmd) {
                return format!("{}{rest}", run_path.display());
            }
        }
    }

    cmd.to_string()
}

async fn run_health_check(cmd: &str, working_dir: &Path) -> HealthOutcome {
    let result = tokio::time::timeout(
        Duration::from_secs(HEALTH_CHECK_TIMEOUT_SECS),
        Command::new("sh")
            .arg("-c")
            .arg(cmd)
            .current_dir(working_dir)
            .output(),
    )
    .await;

    match result {
        Ok(Ok(output)) if output.status.success() => Ok(()),
        Ok(Ok(output)) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            let detail = if stderr.trim().is_empty() {
                &stdout
            } else {
                &stderr
            };
            // Take the last non-empty line — that's usually the summary.
            let summary = detail
                .lines()
                .rev()
                .find(|l| !l.trim().is_empty())
                .unwrap_or("no output");
            let code = output.status.code().unwrap_or(-1);
            Err(format!("exit {code}: {summary}"))
        }
        Ok(Err(e)) => Err(format!("spawn failed: {e}")),
        Err(_) => Err(format!("timed out after {HEALTH_CHECK_TIMEOUT_SECS}s")),
    }
}
