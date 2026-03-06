mod agent;
mod audit;
mod cli;
mod config;
mod env;
mod error;
mod frontmatter;
mod health;
mod manifest;
mod mcp;
mod output;
mod registry;
mod rule;
mod runner;
mod serve;
mod skill;
mod workflow;

use clap::Parser;
use cli::{Cli, Command};
use error::ToolshedError;
use std::process;

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    if let Err(e) = run(cli).await {
        let code = e.exit_code();
        eprintln!("error: {e}");
        process::exit(code);
    }
}

async fn run(cli: Cli) -> Result<(), ToolshedError> {
    match cli.command {
        Command::List { category, health } => {
            cmd_list(category, health).await
        }
        Command::Help { tool, command } => {
            cmd_help(&tool, command.as_deref()).await
        }
        Command::Run {
            tool,
            command,
            args,
            full,
            timeout,
        } => cmd_run(&tool, &command, &args, full, timeout).await,
        Command::Status => {
            eprintln!("status: not yet implemented (v2)");
            Ok(())
        }
        Command::Stop { .. } => {
            eprintln!("stop: not yet implemented (v2)");
            Ok(())
        }
        Command::Validate { tool } => cmd_validate(tool.as_deref()).await,
        Command::AgentPrompt { format } => cmd_agent_prompt(&format).await,
        Command::Skill { action } => cmd_skill(action).await,
        Command::Agent { action } => cmd_agent(action).await,
        Command::Rule { action } => cmd_rule(action).await,
        Command::Workflow { action } => cmd_workflow(action).await,
        Command::Serve { port, category } => serve::serve(port, category).await,
        Command::Audit { action } => cmd_audit(action, cli.own_audit_trail).await,
    }
}

async fn cmd_list(category: Option<String>, show_health: bool) -> Result<(), ToolshedError> {
    let reg = registry::Registry::load()?;

    if let Some(cat) = category {
        let tools = reg
            .by_category
            .get(&cat)
            .ok_or(ToolshedError::CategoryNotFound { name: cat })?;
        for name in tools {
            let tool = &reg.tools[name];
            println!("{:<12} {}", name, tool.manifest.description);
        }
    } else if show_health {
        let health_results = health::check_all(&reg).await;
        for (cat, names) in &reg.by_category {
            let total = names.len();
            let mut up = 0usize;
            let mut down = 0usize;
            let mut unconfigured = 0usize;
            for name in names {
                match health_results.get(name.as_str()) {
                    Some(Some(true)) => up += 1,
                    Some(Some(false)) => down += 1,
                    _ => unconfigured += 1,
                }
            }
            let tool_word = if total == 1 { "tool" } else { "tools" };
            if unconfigured == total {
                println!(
                    "{:<20} {total} {tool_word}  (health not configured)",
                    cat
                );
            } else {
                let parts: Vec<String> = [
                    (up > 0).then(|| format!("{up} up")),
                    (down > 0).then(|| format!("{down} down")),
                ]
                .into_iter()
                .flatten()
                .collect();
                println!(
                    "{:<20} {total} {tool_word}  ({})",
                    cat,
                    parts.join(", ")
                );
            }
        }
    } else {
        for (cat, names) in &reg.by_category {
            let n = names.len();
            let word = if n == 1 { "tool" } else { "tools" };
            println!("{:<20} {n} {word}", cat);
        }
    }

    Ok(())
}

async fn cmd_help(tool_name: &str, command: Option<&str>) -> Result<(), ToolshedError> {
    let reg = registry::Registry::load()?;
    let tool = reg
        .tools
        .get(tool_name)
        .ok_or_else(|| ToolshedError::ToolNotFound {
            name: tool_name.to_string(),
        })?;

    match tool.manifest.tool_type {
        manifest::ToolType::Native => {
            print_native_help(tool, command).await?;
        }
        manifest::ToolType::Mcp => {
            print_mcp_help(tool).await?;
        }
    }

    Ok(())
}

async fn print_native_help(
    tool: &registry::Tool,
    _command_filter: Option<&str>,
) -> Result<(), ToolshedError> {
    let m = &tool.manifest;
    println!("{} — {}", m.name, m.description);
    println!("Type: native");
    println!("Category: {}", m.category);

    if m.health.is_some() {
        let status = health::check_one(tool).await;
        match status {
            Some(true) => println!("Status: up"),
            Some(false) => println!("Status: DOWN"),
            None => {}
        }
    }

    println!();
    println!("Commands:");
    for (cmd_name, cmd) in &m.commands {
        let positionals: Vec<_> = cmd
            .args
            .iter()
            .filter(|(_, a)| a.positional)
            .collect();
        let flags: Vec<_> = cmd
            .args
            .iter()
            .filter(|(_, a)| !a.positional)
            .collect();

        let pos_str: String = positionals
            .iter()
            .map(|(name, a)| {
                if a.required {
                    format!("<{name}>")
                } else {
                    format!("[{name}]")
                }
            })
            .collect::<Vec<_>>()
            .join(" ");

        let flag_str: String = flags
            .iter()
            .map(|(name, _)| format!("[{name}]"))
            .collect::<Vec<_>>()
            .join(" ");

        let usage = [pos_str, flag_str]
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" ");

        println!("  {cmd_name} {usage}");
        println!("    {}", cmd.description);

        for (arg_name, arg) in &cmd.args {
            let req = if arg.required { "required" } else { "optional" };
            let desc = arg.description.as_deref().unwrap_or("");
            if arg.positional {
                println!("    {:<12} {}, {req} — {desc}", arg_name, arg.arg_type);
            } else {
                let default_str = arg
                    .default
                    .as_ref()
                    .map(|v| format!(", default={v}"))
                    .unwrap_or_default();
                println!(
                    "    {:<12} {}{default_str} — {desc}",
                    arg_name, arg.arg_type
                );
            }
        }
        println!();
    }

    println!("Usage: toolshed run {} <command> [args]", m.name);

    Ok(())
}

async fn print_mcp_help(tool: &registry::Tool) -> Result<(), ToolshedError> {
    let m = &tool.manifest;
    let mcp_cfg = m.mcp.as_ref().ok_or_else(|| ToolshedError::MissingMcpConfig {
        tool: m.name.clone(),
    })?;

    println!("{} — {}", m.name, m.description);
    println!("Type: mcp ({})", mcp_cfg.transport);
    println!("Category: {}", m.category);
    println!();

    let tools = mcp::introspect::get_mcp_tools(tool).await?;

    println!("Tools (via MCP):");
    for mcp_tool in &tools {
        let params = mcp_tool.format_params();
        println!("  {}({})", mcp_tool.name, params);
        if let Some(desc) = &mcp_tool.description {
            println!("    {desc}");
        }
        for param in &mcp_tool.params {
            let req = if param.required { "required" } else { "optional" };
            let desc = param.description.as_deref().unwrap_or("");
            println!("    {:<16} {}, {req} — {desc}", param.name, param.param_type);
        }
        println!();
    }

    println!("Usage: toolshed run {} <tool_name> [--arg value ...]", m.name);

    Ok(())
}

async fn cmd_run(
    tool_name: &str,
    command: &str,
    args: &[String],
    full: bool,
    timeout: Option<u64>,
) -> Result<(), ToolshedError> {
    let reg = registry::Registry::load()?;
    let tool = reg
        .tools
        .get(tool_name)
        .ok_or_else(|| ToolshedError::ToolNotFound {
            name: tool_name.to_string(),
        })?;

    // Audit: setup
    let session_id = uuid::Uuid::new_v4().to_string();
    let mut logger = audit::AuditLogger::new(&session_id);

    // Audit: record tool_call
    logger.record(
        "tool",
        "tool_call",
        "user",
        serde_json::json!({
            "tool": tool_name,
            "command": command,
            "args": args,
            "toolType": tool.manifest.tool_type.to_string(),
            "category": tool.manifest.category,
        }),
        None,
    );

    let start = std::time::Instant::now();
    let result = runner::run(tool, command, args, timeout).await;
    let duration_ms = start.elapsed().as_millis() as u64;

    // Audit: record tool_result
    match &result {
        Ok(output) => {
            logger.record(
                "tool",
                "tool_result",
                "user",
                serde_json::json!({
                    "tool": tool_name,
                    "command": command,
                    "durationMs": duration_ms,
                    "outputLen": output.len(),
                }),
                Some("success".to_string()),
            );
        }
        Err(e) => {
            logger.record(
                "tool",
                "tool_result",
                "user",
                serde_json::json!({
                    "tool": tool_name,
                    "command": command,
                    "durationMs": duration_ms,
                    "error": e.to_string(),
                }),
                Some("error".to_string()),
            );
        }
    }

    let result = result?;
    let output_text = if full {
        result
    } else {
        output::truncate(&result, tool.manifest.max_output)
    };

    print!("{output_text}");

    Ok(())
}

async fn cmd_validate(tool_filter: Option<&str>) -> Result<(), ToolshedError> {
    let reg = registry::Registry::load()?;

    let mut has_errors = false;

    if let Some(name) = tool_filter {
        if reg.tools.contains_key(name) {
            println!("{name}  ok");
        } else if let Some((_, err)) = reg.errors.iter().find(|(n, _)| n == name) {
            println!("{name}  ERROR: {err}");
            has_errors = true;
        } else {
            return Err(ToolshedError::ToolNotFound {
                name: name.to_string(),
            });
        }
    } else {
        for name in reg.tools.keys() {
            println!("{name}  ok");
        }
        for (name, err) in &reg.errors {
            println!("{name}  ERROR: {err}");
            has_errors = true;
        }
    }

    if has_errors {
        std::process::exit(3);
    }

    Ok(())
}

async fn cmd_skill(action: cli::SkillAction) -> Result<(), ToolshedError> {
    let reg = skill::SkillRegistry::load()?;

    match action {
        cli::SkillAction::List => {
            for (name, s) in &reg.skills {
                println!("{:<24} {}", name, s.manifest.description);
            }
        }
        cli::SkillAction::Show { name } => {
            let s = reg
                .skills
                .get(&name)
                .ok_or_else(|| ToolshedError::SkillNotFound { name: name.clone() })?;
            print!("{}", s.body);
        }
        cli::SkillAction::Validate { name } => {
            let mut has_errors = false;

            if let Some(name) = name {
                if reg.skills.contains_key(&name) {
                    println!("{name}  ok");
                } else if let Some((_, err)) = reg.errors.iter().find(|(n, _)| n == &name) {
                    println!("{name}  ERROR: {err}");
                    has_errors = true;
                } else {
                    return Err(ToolshedError::SkillNotFound { name });
                }
            } else {
                for name in reg.skills.keys() {
                    println!("{name}  ok");
                }
                for (name, err) in &reg.errors {
                    println!("{name}  ERROR: {err}");
                    has_errors = true;
                }
            }

            if has_errors {
                std::process::exit(3);
            }
        }
    }

    Ok(())
}

async fn cmd_agent(action: cli::AgentAction) -> Result<(), ToolshedError> {
    let reg = agent::AgentRegistry::load()?;

    match action {
        cli::AgentAction::List => {
            for (name, a) in &reg.agents {
                let model_str = a
                    .manifest
                    .model
                    .as_deref()
                    .map(|m| format!("  [{}]", m))
                    .unwrap_or_default();
                println!("{:<24} {}{}", name, a.manifest.description, model_str);
            }
        }
        cli::AgentAction::Show { name } => {
            let a = reg
                .agents
                .get(&name)
                .ok_or_else(|| ToolshedError::AgentNotFound { name: name.clone() })?;
            print!("{}", a.prompt);
        }
        cli::AgentAction::Validate { name } => {
            let mut has_errors = false;

            if let Some(name) = name {
                if reg.agents.contains_key(&name) {
                    println!("{name}  ok");
                } else if let Some((_, err)) = reg.errors.iter().find(|(n, _)| n == &name) {
                    println!("{name}  ERROR: {err}");
                    has_errors = true;
                } else {
                    return Err(ToolshedError::AgentNotFound { name });
                }
            } else {
                for name in reg.agents.keys() {
                    println!("{name}  ok");
                }
                for (name, err) in &reg.errors {
                    println!("{name}  ERROR: {err}");
                    has_errors = true;
                }
            }

            if has_errors {
                std::process::exit(3);
            }
        }
    }

    Ok(())
}

async fn cmd_rule(action: cli::RuleAction) -> Result<(), ToolshedError> {
    let reg = rule::RuleRegistry::load()?;

    match action {
        cli::RuleAction::List => {
            for (name, r) in &reg.rules {
                println!(
                    "{:<24} [{}, {}] {}",
                    name, r.manifest.rule_type, r.manifest.severity, r.manifest.description
                );
            }
        }
        cli::RuleAction::Show { name } => {
            let r = reg
                .rules
                .get(&name)
                .ok_or_else(|| ToolshedError::RuleNotFound { name: name.clone() })?;
            print!("{}", r.body);
        }
        cli::RuleAction::Validate { name } => {
            let mut has_errors = false;

            if let Some(name) = name {
                if reg.rules.contains_key(&name) {
                    println!("{name}  ok");
                } else if let Some((_, err)) = reg.errors.iter().find(|(n, _)| n == &name) {
                    println!("{name}  ERROR: {err}");
                    has_errors = true;
                } else {
                    return Err(ToolshedError::RuleNotFound { name });
                }
            } else {
                for name in reg.rules.keys() {
                    println!("{name}  ok");
                }
                for (name, err) in &reg.errors {
                    println!("{name}  ERROR: {err}");
                    has_errors = true;
                }
            }

            if has_errors {
                std::process::exit(3);
            }
        }
    }

    Ok(())
}

async fn cmd_workflow(action: cli::WorkflowAction) -> Result<(), ToolshedError> {
    let wf_reg = workflow::WorkflowRegistry::load()?;

    match action {
        cli::WorkflowAction::List => {
            for (name, wf) in &wf_reg.workflows {
                println!(
                    "{:<24} {} ({} steps)",
                    name,
                    wf.manifest.description,
                    wf.steps.len()
                );
            }
        }
        cli::WorkflowAction::Show { name } => {
            let wf = wf_reg
                .workflows
                .get(&name)
                .ok_or_else(|| ToolshedError::WorkflowNotFound { name: name.clone() })?;

            println!("{} — {}", wf.manifest.name, wf.manifest.description);
            println!("Timeout: {}s", wf.manifest.timeout);
            println!();
            println!("Steps:");
            for (i, step) in wf.steps.iter().enumerate() {
                let err_marker = if step.continue_on_error { " ?" } else { "" };
                let args_str = if step.args.is_empty() {
                    String::new()
                } else {
                    format!(" {}", step.args.join(" "))
                };
                println!(
                    "  {}. {} {}{}{err_marker}",
                    i + 1,
                    step.tool,
                    step.command,
                    args_str
                );
            }
        }
        cli::WorkflowAction::Validate { name } => {
            let mut has_errors = false;

            if let Some(name) = name {
                if wf_reg.workflows.contains_key(&name) {
                    println!("{name}  ok");
                } else if let Some((_, err)) = wf_reg.errors.iter().find(|(n, _)| n == &name) {
                    println!("{name}  ERROR: {err}");
                    has_errors = true;
                } else {
                    return Err(ToolshedError::WorkflowNotFound { name });
                }
            } else {
                for name in wf_reg.workflows.keys() {
                    println!("{name}  ok");
                }
                for (name, err) in &wf_reg.errors {
                    println!("{name}  ERROR: {err}");
                    has_errors = true;
                }
            }

            if has_errors {
                std::process::exit(3);
            }
        }
        cli::WorkflowAction::Run {
            name,
            full,
            timeout,
            verbose,
        } => {
            let wf = wf_reg
                .workflows
                .get(&name)
                .ok_or_else(|| ToolshedError::WorkflowNotFound { name: name.clone() })?;

            let reg = registry::Registry::load()?;

            // Audit
            let session_id = uuid::Uuid::new_v4().to_string();
            let mut logger = audit::AuditLogger::new(&session_id);
            logger.record(
                "workflow",
                "workflow_run",
                "user",
                serde_json::json!({
                    "workflow": &name,
                    "steps": wf.steps.len(),
                    "timeout": timeout.unwrap_or(wf.manifest.timeout),
                    "verbose": verbose,
                }),
                None,
            );

            let start = std::time::Instant::now();
            let result = workflow::execute(wf, &reg, verbose, full, timeout).await;
            let duration_ms = start.elapsed().as_millis() as u64;

            match &result {
                Ok(output) => {
                    logger.record(
                        "workflow",
                        "workflow_result",
                        "user",
                        serde_json::json!({
                            "workflow": &name,
                            "durationMs": duration_ms,
                            "outputLen": output.len(),
                        }),
                        Some("success".to_string()),
                    );
                    print!("{output}");
                }
                Err(e) => {
                    logger.record(
                        "workflow",
                        "workflow_result",
                        "user",
                        serde_json::json!({
                            "workflow": &name,
                            "durationMs": duration_ms,
                            "error": e.to_string(),
                        }),
                        Some("error".to_string()),
                    );
                    return Err(result.unwrap_err());
                }
            }
        }
    }

    Ok(())
}

async fn cmd_agent_prompt(format: &str) -> Result<(), ToolshedError> {
    let reg = registry::Registry::load()?;
    let skill_reg = skill::SkillRegistry::load()?;
    let agent_reg = agent::AgentRegistry::load()?;
    let rule_reg = rule::RuleRegistry::load()?;
    let wf_reg = workflow::WorkflowRegistry::load()?;

    let mut tool_lines = Vec::new();
    for (cat, names) in &reg.by_category {
        for name in names {
            let tool = &reg.tools[name];
            let m = &tool.manifest;
            let commands = match m.tool_type {
                manifest::ToolType::Native => {
                    m.commands.keys().cloned().collect::<Vec<_>>().join(", ")
                }
                manifest::ToolType::Mcp => "(MCP — run `toolshed help` to discover)".to_string(),
            };
            tool_lines.push(format!(
                "| {:<20} | {:<14} | {:<6} | {} |",
                name,
                cat,
                match m.tool_type {
                    manifest::ToolType::Native => "native",
                    manifest::ToolType::Mcp => "mcp",
                },
                commands
            ));
        }
    }

    let tool_table = tool_lines.join("\n");

    let prompt = format!(
        r#"# Toolshed Agent

You have access to Toolshed, a universal tool registry that provides a consistent interface to native CLI tools and MCP servers.

## Discovery Protocol

Before using any tool, follow these three steps:

1. **List categories** — `toolshed list` shows all available tool categories
2. **List tools in a category** — `toolshed list <category>` shows tools and descriptions
3. **Get tool help** — `toolshed help <tool>` shows commands, parameters, and usage

## Available Tools

| Tool                 | Category       | Type   | Commands |
|----------------------|----------------|--------|----------|
{tool_table}

## Running Tools

```
toolshed run <tool> <command> [args...]
```

- **Native tools**: positional args and `--flag value` pairs
- **MCP tools**: `--name value` pairs (JSON values parsed automatically)
- Use `--full` to disable output truncation
- Use `--timeout <seconds>` for long-running commands

## Key Principles

- **Always discover before using.** Run `toolshed help <tool>` to see exact parameters.
- **Tools are stateless between calls.** Each `toolshed run` is independent.
- **Environment variables** are interpolated at runtime from the shell environment."#
    );

    // Append skills section if any
    let skills_section = if skill_reg.skills.is_empty() {
        String::new()
    } else {
        let mut lines = Vec::new();
        lines.push("\n\n## Available Skills\n".to_string());
        lines.push("| Skill                  | Description |".to_string());
        lines.push("|------------------------|-------------|".to_string());
        for (name, s) in &skill_reg.skills {
            lines.push(format!("| {:<22} | {} |", name, s.manifest.description));
        }
        lines.push("\nUse `toolshed skill show <name>` to read a skill's full content.".to_string());
        lines.join("\n")
    };

    // Append agents section if any
    let agents_section = if agent_reg.agents.is_empty() {
        String::new()
    } else {
        let mut lines = Vec::new();
        lines.push("\n\n## Available Agents\n".to_string());
        lines.push("| Agent                  | Description | Model |".to_string());
        lines.push("|------------------------|-------------|-------|".to_string());
        for (name, a) in &agent_reg.agents {
            let model = a.manifest.model.as_deref().unwrap_or("-");
            lines.push(format!("| {:<22} | {} | {} |", name, a.manifest.description, model));
        }
        lines.push("\nUse `toolshed agent show <name>` to read an agent's system prompt.".to_string());
        lines.join("\n")
    };

    // Rules section: full body injected (mandatory constraints)
    let rules_section = if rule_reg.rules.is_empty() {
        String::new()
    } else {
        let mut lines = Vec::new();
        lines.push("\n\n## Rules\n".to_string());
        lines.push("**The following rules MUST be followed:**\n".to_string());
        for (name, r) in &rule_reg.rules {
            let scope_str = r.manifest.scope.join(", ");
            lines.push(format!(
                "### {} ({}, {}, {})\n",
                name, r.manifest.rule_type, r.manifest.severity, scope_str
            ));
            lines.push(r.body.clone());
            lines.push(String::new());
        }
        lines.join("\n")
    };

    // Append workflows section if any
    let workflows_section = if wf_reg.workflows.is_empty() {
        String::new()
    } else {
        let mut lines = Vec::new();
        lines.push("\n\n## Available Workflows\n".to_string());
        lines.push("| Workflow               | Steps | Description |".to_string());
        lines.push("|------------------------|-------|-------------|".to_string());
        for (name, wf) in &wf_reg.workflows {
            lines.push(format!(
                "| {:<22} | {:<5} | {} |",
                name,
                wf.steps.len(),
                wf.manifest.description
            ));
        }
        lines.push("\nUse `toolshed workflow show <name>` to see steps. Use `toolshed workflow run <name>` to execute.".to_string());
        lines.join("\n")
    };

    let prompt = format!("{prompt}{skills_section}{agents_section}{rules_section}{workflows_section}");

    match format {
        "skill" => {
            println!("---");
            println!("name: toolshed");
            println!("description: Universal tool registry — discover and run CLI tools and MCP servers");
            println!("user_invocable: true");
            println!("---");
            println!();
            println!("{prompt}");
        }
        _ => {
            println!("{prompt}");
        }
    }

    Ok(())
}

async fn cmd_audit(
    action: cli::AuditAction,
    own_audit_trail: bool,
) -> Result<(), ToolshedError> {
    if !own_audit_trail {
        return cmd_audit_exec(action).await;
    }

    // Meta-audit: log the audit operation itself
    let session_id = uuid::Uuid::new_v4().to_string();
    let mut logger = audit::AuditLogger::new(&session_id);

    let (event_name, event_data) = audit_action_metadata(&action);
    logger.record("audit", event_name, "user", event_data, None);

    let start = std::time::Instant::now();
    let result = cmd_audit_exec(action).await;
    let duration_ms = start.elapsed().as_millis() as u64;

    match &result {
        Ok(()) => {
            logger.record(
                "audit",
                "audit_result",
                "user",
                serde_json::json!({
                    "action": event_name,
                    "durationMs": duration_ms,
                }),
                Some("success".to_string()),
            );
        }
        Err(e) => {
            logger.record(
                "audit",
                "audit_result",
                "user",
                serde_json::json!({
                    "action": event_name,
                    "durationMs": duration_ms,
                    "error": e.to_string(),
                }),
                Some("error".to_string()),
            );
        }
    }

    result
}

fn audit_action_metadata(action: &cli::AuditAction) -> (&'static str, serde_json::Value) {
    match action {
        cli::AuditAction::List { limit } => (
            "audit_list",
            serde_json::json!({"action": "list", "limit": limit}),
        ),
        cli::AuditAction::Verify { session } => (
            "audit_verify",
            serde_json::json!({"action": "verify", "session": session}),
        ),
        cli::AuditAction::Query {
            session,
            event,
            outcome,
            since,
            search,
            limit,
        } => (
            "audit_query",
            serde_json::json!({
                "action": "query",
                "session": session,
                "event": event,
                "outcome": outcome,
                "since": since,
                "search": search,
                "limit": limit,
            }),
        ),
    }
}

async fn cmd_audit_exec(action: cli::AuditAction) -> Result<(), ToolshedError> {
    let dir = config::audit_dir();

    match action {
        cli::AuditAction::List { limit } => {
            let files = audit::list_files(&dir);
            if files.is_empty() {
                println!("No audit files found.");
                return Ok(());
            }
            for file in files.iter().take(limit) {
                println!(
                    "{}  {}  {} entries  {} bytes",
                    file.session_id, file.date, file.entry_count, file.size_bytes
                );
            }
        }
        cli::AuditAction::Verify { session } => {
            let files = audit::list_files(&dir);
            let to_verify: Vec<_> = if let Some(ref sid) = session {
                files.into_iter().filter(|f| f.session_id.contains(sid)).collect()
            } else {
                files
            };

            if to_verify.is_empty() {
                println!("No audit files to verify.");
                return Ok(());
            }

            let mut all_valid = true;
            for file in &to_verify {
                let result = audit::verify_file(&file.path);
                if result.valid {
                    println!(
                        "VALID  {}  ({} entries)",
                        file.session_id, result.total_entries
                    );
                } else {
                    all_valid = false;
                    println!(
                        "BROKEN {}  {}",
                        file.session_id,
                        result.error_message.as_deref().unwrap_or("unknown error")
                    );
                }
            }

            if !all_valid {
                return Err(ToolshedError::AuditChainBroken {
                    message: "one or more audit files have broken hash chains".to_string(),
                });
            }
        }
        cli::AuditAction::Query {
            session,
            event,
            outcome,
            since,
            search,
            limit,
        } => {
            let files = audit::list_files(&dir);
            let to_query: Vec<_> = if let Some(ref sid) = session {
                files.into_iter().filter(|f| f.session_id.contains(sid)).collect()
            } else {
                files
            };

            let options = audit::QueryOptions {
                event,
                outcome,
                since,
                search,
                limit,
            };

            let mut all_entries = Vec::new();
            for file in &to_query {
                let entries = audit::query_entries(&file.path, &options);
                all_entries.extend(entries);
            }

            // Re-sort by ts descending and apply limit
            all_entries.sort_by(|a, b| b.ts.cmp(&a.ts));
            all_entries.truncate(limit);

            for entry in &all_entries {
                println!("{}", serde_json::to_string(entry).unwrap_or_default());
            }
        }
    }

    Ok(())
}
