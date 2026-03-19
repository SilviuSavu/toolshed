use std::fmt;
use std::io;

#[derive(Debug)]
pub enum ToolshedError {
    NoToolshedDir { path: String },
    ToolNotFound { name: String },
    CategoryNotFound { name: String },
    BadManifest { tool: String, reason: String },
    MissingRunScript { tool: String },
    MissingMcpConfig { tool: String },
    CommandNotFound { tool: String, command: String },
    MissingArg { tool: String, command: String, arg: String },
    ToolFailed { tool: String, code: i32, stderr: String },
    ToolTimeout { tool: String, timeout_secs: u64 },
    McpSpawnFailed { tool: String, reason: String },
    McpRpcError { tool: String, code: i64, message: String },
    McpBadResponse { tool: String, reason: String },
    McpCrashed { tool: String },
    McpHttpError { tool: String, reason: String },
    SkillNotFound { name: String },
    AgentNotFound { name: String },
    BadSkill { skill: String, reason: String },
    BadAgent { agent: String, reason: String },
    RuleNotFound { name: String },
    BadRule { rule: String, reason: String },
    WorkflowNotFound { name: String },
    BadWorkflow { workflow: String, reason: String },
    WorkflowStepFailed { workflow: String, step: usize, tool: String, command: String, reason: String },
    WorkflowTimeout { workflow: String, timeout_secs: u64 },
    AuditChainBroken { message: String },
    EnvVarNotSet { var: String },
    VaultError { reason: String },
    VaultAuthFailed { reason: String },
    RecoveryExhausted { tool: String, reason: String },
    HealthProbeFailed { tool: String, reason: String },
    Io(io::Error),
    Json(serde_json::Error),
    Http(reqwest::Error),
}

impl fmt::Display for ToolshedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoToolshedDir { path } => write!(f, "no toolshed directory at {path}"),
            Self::ToolNotFound { name } => write!(f, "tool not found: {name}"),
            Self::CategoryNotFound { name } => write!(f, "category not found: {name}"),
            Self::BadManifest { tool, reason } => write!(f, "bad manifest for tool '{tool}': {reason}"),
            Self::MissingRunScript { tool } => write!(f, "missing 'run' script for tool '{tool}'"),
            Self::MissingMcpConfig { tool } => write!(f, "missing mcp config for tool '{tool}'"),
            Self::CommandNotFound { tool, command } => write!(f, "command not found: {tool}/{command}"),
            Self::MissingArg { tool, command, arg } => write!(f, "missing required argument '{arg}' for {tool}/{command}"),
            Self::ToolFailed { tool, code, stderr } => write!(f, "tool '{tool}' failed with exit code {code}: {stderr}"),
            Self::ToolTimeout { tool, timeout_secs } => write!(f, "tool '{tool}' timed out after {timeout_secs}s"),
            Self::McpSpawnFailed { tool, reason } => write!(f, "failed to spawn MCP server for '{tool}': {reason}"),
            Self::McpRpcError { tool, code, message } => write!(f, "MCP RPC error for '{tool}': [{code}] {message}"),
            Self::McpBadResponse { tool, reason } => write!(f, "MCP bad response for '{tool}': {reason}"),
            Self::McpCrashed { tool } => write!(f, "MCP server crashed for '{tool}'"),
            Self::McpHttpError { tool, reason } => write!(f, "MCP HTTP error for '{tool}': {reason}"),
            Self::SkillNotFound { name } => write!(f, "skill not found: {name}"),
            Self::AgentNotFound { name } => write!(f, "agent not found: {name}"),
            Self::BadSkill { skill, reason } => write!(f, "bad skill '{skill}': {reason}"),
            Self::BadAgent { agent, reason } => write!(f, "bad agent '{agent}': {reason}"),
            Self::RuleNotFound { name } => write!(f, "rule not found: {name}"),
            Self::BadRule { rule, reason } => write!(f, "bad rule '{rule}': {reason}"),
            Self::WorkflowNotFound { name } => write!(f, "workflow not found: {name}"),
            Self::BadWorkflow { workflow, reason } => write!(f, "bad workflow '{workflow}': {reason}"),
            Self::WorkflowStepFailed { workflow, step, tool, command, reason } => {
                write!(f, "workflow '{workflow}' step {step} ({tool} {command}) failed: {reason}")
            }
            Self::WorkflowTimeout { workflow, timeout_secs } => write!(f, "workflow '{workflow}' timed out after {timeout_secs}s"),
            Self::AuditChainBroken { message } => write!(f, "audit chain broken: {message}"),
            Self::EnvVarNotSet { var } => write!(f, "environment variable not set: {var}"),
            Self::VaultError { reason } => write!(f, "vault request failed: {reason}"),
            Self::VaultAuthFailed { reason } => write!(f, "vault auth failed: {reason}"),
            Self::RecoveryExhausted { tool, reason } => write!(f, "tool recovery exhausted for '{tool}': {reason}"),
            Self::HealthProbeFailed { tool, reason } => write!(f, "daemon health probe failed for '{tool}': {reason}"),
            Self::Io(err) => write!(f, "I/O error: {err}"),
            Self::Json(err) => write!(f, "JSON error: {err}"),
            Self::Http(err) => write!(f, "HTTP error: {err}"),
        }
    }
}

impl std::error::Error for ToolshedError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(err) => Some(err),
            Self::Json(err) => Some(err),
            Self::Http(err) => Some(err),
            _ => None,
        }
    }
}

impl From<io::Error> for ToolshedError {
    fn from(err: io::Error) -> Self {
        Self::Io(err)
    }
}

impl From<serde_json::Error> for ToolshedError {
    fn from(err: serde_json::Error) -> Self {
        Self::Json(err)
    }
}

impl From<reqwest::Error> for ToolshedError {
    fn from(err: reqwest::Error) -> Self {
        Self::Http(err)
    }
}

impl ToolshedError {
    pub const fn exit_code(&self) -> i32 {
        match self {
            Self::ToolNotFound { .. }
            | Self::CategoryNotFound { .. }
            | Self::CommandNotFound { .. }
            | Self::MissingArg { .. }
            | Self::SkillNotFound { .. }
            | Self::AgentNotFound { .. }
            | Self::RuleNotFound { .. }
            | Self::WorkflowNotFound { .. } => 1,

            Self::ToolFailed { code, .. } if *code != 0 => *code,
            Self::ToolFailed { .. }
            | Self::McpRpcError { .. }
            | Self::McpCrashed { .. }
            | Self::McpHttpError { .. }
            | Self::WorkflowStepFailed { .. }
            | Self::HealthProbeFailed { .. } => 2,

            Self::BadManifest { .. }
            | Self::MissingRunScript { .. }
            | Self::MissingMcpConfig { .. }
            | Self::NoToolshedDir { .. }
            | Self::BadSkill { .. }
            | Self::BadAgent { .. }
            | Self::BadRule { .. }
            | Self::BadWorkflow { .. } => 3,

            Self::ToolTimeout { .. } | Self::WorkflowTimeout { .. } => 4,

            Self::AuditChainBroken { .. } | Self::EnvVarNotSet { .. } => 5,

            Self::VaultError { .. } | Self::VaultAuthFailed { .. } => 6,
            Self::RecoveryExhausted { .. } => 7,

            _ => 99,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ToolshedError;

    #[test]
    fn display_no_toolshed_dir() {
        let e = ToolshedError::NoToolshedDir {
            path: "/tmp/ts".into(),
        };
        assert_eq!(e.to_string(), "no toolshed directory at /tmp/ts");
    }

    #[test]
    fn display_tool_not_found() {
        let e = ToolshedError::ToolNotFound {
            name: "grep".into(),
        };
        assert_eq!(e.to_string(), "tool not found: grep");
    }

    #[test]
    fn display_bad_manifest() {
        let e = ToolshedError::BadManifest {
            tool: "foo".into(),
            reason: "missing name".into(),
        };
        assert_eq!(
            e.to_string(),
            "bad manifest for tool 'foo': missing name"
        );
    }

    #[test]
    fn display_missing_arg() {
        let e = ToolshedError::MissingArg {
            tool: "echo".into(),
            command: "say".into(),
            arg: "text".into(),
        };
        assert_eq!(
            e.to_string(),
            "missing required argument 'text' for echo/say"
        );
    }

    #[test]
    fn display_tool_failed() {
        let e = ToolshedError::ToolFailed {
            tool: "lint".into(),
            code: 1,
            stderr: "bad code".into(),
        };
        assert_eq!(
            e.to_string(),
            "tool 'lint' failed with exit code 1: bad code"
        );
    }

    #[test]
    fn display_mcp_rpc_error() {
        let e = ToolshedError::McpRpcError {
            tool: "search".into(),
            code: -32600,
            message: "invalid".into(),
        };
        assert_eq!(
            e.to_string(),
            "MCP RPC error for 'search': [-32600] invalid"
        );
    }

    #[test]
    fn display_workflow_step_failed() {
        let e = ToolshedError::WorkflowStepFailed {
            workflow: "ci".into(),
            step: 2,
            tool: "lint".into(),
            command: "run".into(),
            reason: "failed".into(),
        };
        assert_eq!(
            e.to_string(),
            "workflow 'ci' step 2 (lint run) failed: failed"
        );
    }

    #[test]
    fn display_io_error() {
        let inner = std::io::Error::other("gone");
        let e = ToolshedError::Io(inner);
        assert_eq!(e.to_string(), "I/O error: gone");
    }

    #[test]
    fn display_mcp_crashed() {
        let e = ToolshedError::McpCrashed {
            tool: "srv".into(),
        };
        assert_eq!(e.to_string(), "MCP server crashed for 'srv'");
    }

    #[test]
    fn from_io_error() {
        let io_err = std::io::Error::other("disk full");
        let e: ToolshedError = io_err.into();
        assert!(matches!(e, ToolshedError::Io(_)));
        assert_eq!(e.to_string(), "I/O error: disk full");
    }

    #[test]
    fn from_json_error() {
        let Err(json_err) = serde_json::from_str::<serde_json::Value>("{{bad") else {
            unreachable!();
        };
        let msg = format!("JSON error: {json_err}");
        let e: ToolshedError = json_err.into();
        assert!(matches!(e, ToolshedError::Json(_)));
        assert_eq!(e.to_string(), msg);
    }

    #[test]
    fn exit_code_not_found_variants() {
        assert_eq!(
            ToolshedError::ToolNotFound { name: "x".into() }.exit_code(),
            1
        );
        assert_eq!(
            ToolshedError::CategoryNotFound { name: "x".into() }.exit_code(),
            1
        );
        assert_eq!(
            ToolshedError::SkillNotFound { name: "x".into() }.exit_code(),
            1
        );
        assert_eq!(
            ToolshedError::WorkflowNotFound { name: "x".into() }.exit_code(),
            1
        );
    }

    #[test]
    fn exit_code_tool_failed_uses_code() {
        let e = ToolshedError::ToolFailed {
            tool: "x".into(),
            code: 42,
            stderr: String::new(),
        };
        assert_eq!(e.exit_code(), 42);
    }

    #[test]
    fn exit_code_bad_manifest() {
        let e = ToolshedError::BadManifest {
            tool: "x".into(),
            reason: "y".into(),
        };
        assert_eq!(e.exit_code(), 3);
    }

    #[test]
    fn exit_code_timeout() {
        let e = ToolshedError::ToolTimeout {
            tool: "x".into(),
            timeout_secs: 30,
        };
        assert_eq!(e.exit_code(), 4);
    }

    #[test]
    fn exit_code_vault() {
        assert_eq!(
            ToolshedError::VaultError { reason: "x".into() }.exit_code(),
            6
        );
        assert_eq!(
            ToolshedError::VaultAuthFailed { reason: "x".into() }.exit_code(),
            6
        );
    }

    #[test]
    fn exit_code_io_is_wildcard() {
        let e = ToolshedError::Io(std::io::Error::other("x"));
        assert_eq!(e.exit_code(), 99);
    }

    #[test]
    fn error_trait_source_io() {
        use std::error::Error;
        let e = ToolshedError::Io(std::io::Error::other("x"));
        assert!(e.source().is_some());
    }

    #[test]
    fn error_trait_source_none_for_plain_variants() {
        use std::error::Error;
        let e = ToolshedError::ToolNotFound { name: "x".into() };
        assert!(e.source().is_none());
    }
}
