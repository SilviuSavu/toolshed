use std::io;

#[derive(Debug, thiserror::Error)]
pub enum ToolshedError {
    #[error("no toolshed directory at {path}")]
    NoToolshedDir { path: String },

    #[error("tool not found: {name}")]
    ToolNotFound { name: String },

    #[error("category not found: {name}")]
    CategoryNotFound { name: String },

    #[error("bad manifest for tool '{tool}': {reason}")]
    BadManifest { tool: String, reason: String },

    #[error("missing 'run' script for tool '{tool}'")]
    MissingRunScript { tool: String },

    #[error("missing mcp config for tool '{tool}'")]
    MissingMcpConfig { tool: String },

    #[error("command not found: {tool}/{command}")]
    CommandNotFound { tool: String, command: String },

    #[error("missing required argument '{arg}' for {tool}/{command}")]
    MissingArg {
        tool: String,
        command: String,
        arg: String,
    },

    #[error("tool '{tool}' failed with exit code {code}: {stderr}")]
    ToolFailed {
        tool: String,
        code: i32,
        stderr: String,
    },

    #[error("tool '{tool}' timed out after {timeout_secs}s")]
    ToolTimeout { tool: String, timeout_secs: u64 },

    #[error("failed to spawn MCP server for '{tool}': {reason}")]
    McpSpawnFailed { tool: String, reason: String },

    #[error("MCP initialization failed for '{tool}': {reason}")]
    McpInitFailed { tool: String, reason: String },

    #[error("MCP RPC error for '{tool}': [{code}] {message}")]
    McpRpcError {
        tool: String,
        code: i64,
        message: String,
    },

    #[error("MCP bad response for '{tool}': {reason}")]
    McpBadResponse { tool: String, reason: String },

    #[error("MCP server crashed for '{tool}'")]
    McpCrashed { tool: String },

    #[error("MCP HTTP error for '{tool}': {reason}")]
    McpHttpError { tool: String, reason: String },

    #[error("skill not found: {name}")]
    SkillNotFound { name: String },

    #[error("agent not found: {name}")]
    AgentNotFound { name: String },

    #[error("bad skill '{skill}': {reason}")]
    BadSkill { skill: String, reason: String },

    #[error("bad agent '{agent}': {reason}")]
    BadAgent { agent: String, reason: String },

    #[error("rule not found: {name}")]
    RuleNotFound { name: String },

    #[error("bad rule '{rule}': {reason}")]
    BadRule { rule: String, reason: String },

    #[error("workflow not found: {name}")]
    WorkflowNotFound { name: String },

    #[error("bad workflow '{workflow}': {reason}")]
    BadWorkflow { workflow: String, reason: String },

    #[error("workflow '{workflow}' step {step} ({tool} {command}) failed: {reason}")]
    WorkflowStepFailed {
        workflow: String,
        step: usize,
        tool: String,
        command: String,
        reason: String,
    },

    #[error("workflow '{workflow}' timed out after {timeout_secs}s")]
    WorkflowTimeout { workflow: String, timeout_secs: u64 },

    #[error("audit chain broken: {message}")]
    AuditChainBroken { message: String },

    #[error("environment variable not set: {var}")]
    EnvVarNotSet { var: String },

    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
}

impl ToolshedError {
    pub fn exit_code(&self) -> i32 {
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
            | Self::WorkflowStepFailed { .. } => 2,

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

            _ => 99,
        }
    }
}
