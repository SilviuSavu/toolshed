use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::config;
use crate::error::ToolshedError;
use crate::frontmatter;
use crate::manifest;
use crate::registry;
use crate::runner;

#[derive(Debug, Clone)]
pub struct WorkflowManifest {
    pub name: String,
    pub description: String,
    pub timeout: u64,
}

#[derive(Debug, Clone)]
pub struct Step {
    pub tool: String,
    pub command: String,
    pub args: Vec<String>,
    pub continue_on_error: bool,
    pub line_number: usize,
}

#[derive(Debug, Clone)]
pub struct Workflow {
    pub dir: PathBuf,
    pub manifest: WorkflowManifest,
    pub steps: Vec<Step>,
    pub body: String,
}

pub struct WorkflowRegistry {
    pub workflows: BTreeMap<String, Workflow>,
    pub errors: Vec<(String, String)>,
}

impl WorkflowRegistry {
    pub fn load() -> Result<Self, ToolshedError> {
        let dir = config::workflows_dir();
        let mut workflows = BTreeMap::new();
        let mut errors = Vec::new();

        if !dir.exists() {
            return Ok(Self { workflows, errors });
        }

        let entries = std::fs::read_dir(&dir).map_err(ToolshedError::Io)?;

        for entry in entries {
            let entry = entry.map_err(ToolshedError::Io)?;
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }

            let dir_name = match entry.file_name().to_str() {
                Some(n) => n.to_string(),
                None => continue,
            };

            match load_workflow(&path, &dir_name) {
                Ok(wf) => {
                    workflows.insert(dir_name, wf);
                }
                Err(e) => {
                    errors.push((dir_name, e.to_string()));
                }
            }
        }

        Ok(Self { workflows, errors })
    }
}

fn load_workflow(dir: &PathBuf, dir_name: &str) -> Result<Workflow, ToolshedError> {
    let wf_md = dir.join("WORKFLOW.md");

    if !wf_md.exists() {
        return Err(ToolshedError::BadWorkflow {
            workflow: dir_name.to_string(),
            reason: "missing WORKFLOW.md".to_string(),
        });
    }

    let content = std::fs::read_to_string(&wf_md).map_err(|e| ToolshedError::BadWorkflow {
        workflow: dir_name.to_string(),
        reason: format!("cannot read WORKFLOW.md: {e}"),
    })?;

    let (meta, body) = frontmatter::parse(&content)?;

    let name = meta.get("name").ok_or_else(|| ToolshedError::BadWorkflow {
        workflow: dir_name.to_string(),
        reason: "frontmatter missing 'name'".to_string(),
    })?;

    let description = meta
        .get("description")
        .ok_or_else(|| ToolshedError::BadWorkflow {
            workflow: dir_name.to_string(),
            reason: "frontmatter missing 'description'".to_string(),
        })?;

    // Validate name matches directory
    if name != dir_name {
        return Err(ToolshedError::BadWorkflow {
            workflow: dir_name.to_string(),
            reason: format!("name '{}' does not match directory '{dir_name}'", name),
        });
    }

    // Validate name format
    if !manifest::is_valid_name(name) {
        return Err(ToolshedError::BadWorkflow {
            workflow: dir_name.to_string(),
            reason: format!("name must be 1-64 chars, [a-z0-9_-] only, got '{}'", name),
        });
    }

    // Validate description
    if description.is_empty() {
        return Err(ToolshedError::BadWorkflow {
            workflow: dir_name.to_string(),
            reason: "description cannot be empty".to_string(),
        });
    }
    if description.len() > 300 {
        return Err(ToolshedError::BadWorkflow {
            workflow: dir_name.to_string(),
            reason: "description exceeds 300 chars".to_string(),
        });
    }

    // Parse timeout
    let timeout = match meta.get("timeout") {
        Some(t) => t.parse::<u64>().map_err(|_| ToolshedError::BadWorkflow {
            workflow: dir_name.to_string(),
            reason: format!("timeout must be a positive integer, got '{t}'"),
        })?,
        None => config::DEFAULT_WORKFLOW_TIMEOUT_SECS,
    };
    if timeout == 0 {
        return Err(ToolshedError::BadWorkflow {
            workflow: dir_name.to_string(),
            reason: "timeout must be positive".to_string(),
        });
    }

    let steps = parse_steps(&body, dir_name)?;

    if steps.is_empty() {
        return Err(ToolshedError::BadWorkflow {
            workflow: dir_name.to_string(),
            reason: "workflow has no steps".to_string(),
        });
    }

    Ok(Workflow {
        dir: dir.clone(),
        manifest: WorkflowManifest {
            name: name.clone(),
            description: description.clone(),
            timeout,
        },
        steps,
        body: body.to_string(),
    })
}

/// Split a line into tokens respecting double quotes.
pub fn shell_split(line: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut chars = line.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '"' {
            in_quotes = !in_quotes;
        } else if c == ' ' && !in_quotes {
            if !current.is_empty() {
                tokens.push(std::mem::take(&mut current));
            }
        } else {
            current.push(c);
        }
    }

    if !current.is_empty() {
        tokens.push(current);
    }

    tokens
}

/// Parse body text into workflow steps.
pub fn parse_steps(body: &str, workflow_name: &str) -> Result<Vec<Step>, ToolshedError> {
    let mut steps = Vec::new();

    for (i, line) in body.lines().enumerate() {
        let trimmed = line.trim();

        // Skip empty lines and comments
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        // Check for continue-on-error marker
        let (step_line, continue_on_error) = if trimmed.ends_with(" ?") {
            (&trimmed[..trimmed.len() - 2], true)
        } else {
            (trimmed, false)
        };

        let tokens = shell_split(step_line);
        if tokens.is_empty() {
            continue;
        }

        if tokens.len() < 2 {
            return Err(ToolshedError::BadWorkflow {
                workflow: workflow_name.to_string(),
                reason: format!("line {}: step must have at least a tool and command", i + 1),
            });
        }

        steps.push(Step {
            tool: tokens[0].clone(),
            command: tokens[1].clone(),
            args: tokens[2..].to_vec(),
            continue_on_error,
            line_number: i + 1,
        });
    }

    Ok(steps)
}

/// Replace `${prev}` in a string with the given value.
fn substitute_prev(input: &str, prev: &str) -> String {
    input.replace("${prev}", prev)
}

/// Execute a workflow: run each step sequentially, passing stdout via `${prev}`.
pub async fn execute(
    workflow: &Workflow,
    reg: &registry::Registry,
    verbose: bool,
    full: bool,
    timeout_override: Option<u64>,
) -> Result<String, ToolshedError> {
    let total_timeout = timeout_override.unwrap_or(workflow.manifest.timeout);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(total_timeout);

    let mut prev = String::new();

    for (i, step) in workflow.steps.iter().enumerate() {
        // Check workflow-wide timeout
        let remaining = deadline
            .checked_duration_since(std::time::Instant::now())
            .unwrap_or_default();
        if remaining.is_zero() {
            return Err(ToolshedError::WorkflowTimeout {
                workflow: workflow.manifest.name.clone(),
                timeout_secs: total_timeout,
            });
        }

        // Substitute ${prev} in tool, command, and args, then env interpolate
        let tool_name = crate::env::interpolate(&substitute_prev(&step.tool, &prev))?;
        let command = crate::env::interpolate(&substitute_prev(&step.command, &prev))?;
        let args: Vec<String> = step
            .args
            .iter()
            .map(|a| {
                let substituted = substitute_prev(a, &prev);
                crate::env::interpolate(&substituted)
            })
            .collect::<Result<Vec<_>, _>>()?;

        let tool = reg
            .tools
            .get(&tool_name)
            .ok_or_else(|| ToolshedError::WorkflowStepFailed {
                workflow: workflow.manifest.name.clone(),
                step: i + 1,
                tool: tool_name.clone(),
                command: command.clone(),
                reason: format!("tool '{}' not found", tool_name),
            })?;

        let step_timeout = Some(remaining.as_secs());

        if verbose {
            eprintln!(
                "--- step {} of {}: {} {} {} ---",
                i + 1,
                workflow.steps.len(),
                tool_name,
                command,
                args.join(" ")
            );
        }

        match runner::run(tool, &command, &args, step_timeout).await {
            Ok(output) => {
                let output = if full {
                    output
                } else {
                    crate::output::truncate(&output, tool.manifest.max_output)
                };
                if verbose {
                    eprint!("{output}");
                }
                prev = output.trim().to_string();
            }
            Err(e) => {
                if step.continue_on_error {
                    if verbose {
                        eprintln!("warning: step {} failed (continue-on-error): {e}", i + 1);
                    }
                    prev = String::new();
                } else {
                    return Err(ToolshedError::WorkflowStepFailed {
                        workflow: workflow.manifest.name.clone(),
                        step: i + 1,
                        tool: tool_name,
                        command,
                        reason: e.to_string(),
                    });
                }
            }
        }
    }

    Ok(if prev.is_empty() {
        prev
    } else {
        format!("{prev}\n")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_split_simple() {
        assert_eq!(shell_split("echo say hello"), vec!["echo", "say", "hello"]);
    }

    #[test]
    fn shell_split_quoted() {
        assert_eq!(
            shell_split(r#"echo say "hello world""#),
            vec!["echo", "say", "hello world"]
        );
    }

    #[test]
    fn shell_split_empty() {
        assert!(shell_split("").is_empty());
    }

    #[test]
    fn shell_split_extra_spaces() {
        assert_eq!(
            shell_split("  echo   say   hello  "),
            vec!["echo", "say", "hello"]
        );
    }

    #[test]
    fn parse_steps_basic() {
        let body = "echo say hello\necho say world\n";
        let steps = parse_steps(body, "test").unwrap();
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0].tool, "echo");
        assert_eq!(steps[0].command, "say");
        assert_eq!(steps[0].args, vec!["hello"]);
        assert_eq!(steps[1].args, vec!["world"]);
    }

    #[test]
    fn parse_steps_comments_and_blanks() {
        let body = "# comment\n\necho say hello\n\n# another\necho say bye\n";
        let steps = parse_steps(body, "test").unwrap();
        assert_eq!(steps.len(), 2);
    }

    #[test]
    fn parse_steps_continue_on_error() {
        let body = "echo say hello ?\necho say world\n";
        let steps = parse_steps(body, "test").unwrap();
        assert!(steps[0].continue_on_error);
        assert!(!steps[1].continue_on_error);
    }

    #[test]
    fn parse_steps_line_numbers() {
        let body = "# comment\necho say hello\n\necho say world\n";
        let steps = parse_steps(body, "test").unwrap();
        assert_eq!(steps[0].line_number, 2);
        assert_eq!(steps[1].line_number, 4);
    }

    #[test]
    fn parse_steps_too_few_tokens() {
        let body = "echo\n";
        let err = parse_steps(body, "test").unwrap_err();
        assert!(err.to_string().contains("at least a tool and command"));
    }

    #[test]
    fn parse_steps_prev_in_args() {
        let body = "echo say ${prev}\n";
        let steps = parse_steps(body, "test").unwrap();
        assert_eq!(steps[0].args, vec!["${prev}"]);
    }

    #[test]
    fn substitute_prev_works() {
        assert_eq!(
            substitute_prev("hello ${prev} world", "foo"),
            "hello foo world"
        );
        assert_eq!(substitute_prev("no var", "foo"), "no var");
    }
}
