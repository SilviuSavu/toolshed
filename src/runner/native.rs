use crate::config::DEFAULT_TOOL_TIMEOUT_SECS;
use crate::error::ToolshedError;
use crate::registry::Tool;
use std::time::Duration;
use tokio::process::Command;

pub async fn run(
    tool: &Tool,
    command: &str,
    args: &[String],
    timeout: Option<u64>,
) -> Result<String, ToolshedError> {
    let manifest = &tool.manifest;

    // Validate the command exists
    if !manifest.commands.contains_key(command) {
        return Err(ToolshedError::CommandNotFound {
            tool: manifest.name.clone(),
            command: command.to_string(),
        });
    }

    let cmd_def = &manifest.commands[command];

    // Validate required positional args
    let positionals: Vec<(&String, &crate::manifest::ArgDef)> = cmd_def
        .args
        .iter()
        .filter(|(_, a)| a.positional && a.required)
        .collect();

    // Count how many positional args we have in the input
    // (non-flag args, i.e. args not starting with --)
    let provided_positional: Vec<&String> = args.iter().filter(|a| !a.starts_with("--")).collect();

    if provided_positional.len() < positionals.len() {
        let missing = &positionals[provided_positional.len()];
        return Err(ToolshedError::MissingArg {
            tool: manifest.name.clone(),
            command: command.to_string(),
            arg: missing.0.clone(),
        });
    }

    let run_path = tool
        .run_path
        .as_ref()
        .ok_or_else(|| ToolshedError::MissingRunScript {
            tool: manifest.name.clone(),
        })?;

    let timeout_secs = timeout.unwrap_or(DEFAULT_TOOL_TIMEOUT_SECS);

    let mut cmd = Command::new(run_path);
    cmd.arg(command);
    cmd.args(args);
    cmd.current_dir(&tool.dir);

    let result = tokio::time::timeout(Duration::from_secs(timeout_secs), cmd.output()).await;

    match result {
        Ok(Ok(output)) => {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();

            if output.status.success() {
                Ok(stdout)
            } else {
                let code = output.status.code().unwrap_or(1);
                Err(ToolshedError::ToolFailed {
                    tool: manifest.name.clone(),
                    code,
                    stderr: if stderr.is_empty() { stdout } else { stderr },
                })
            }
        }
        Ok(Err(e)) => Err(ToolshedError::Io(e)),
        Err(_) => Err(ToolshedError::ToolTimeout {
            tool: manifest.name.clone(),
            timeout_secs,
        }),
    }
}
