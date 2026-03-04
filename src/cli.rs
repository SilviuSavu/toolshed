use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "toolshed",
    about = "Universal tool registry & executor",
    disable_help_subcommand = true
)]
pub struct Cli {
    /// Log audit subcommands to the audit trail (audit the auditor)
    #[arg(long, global = true)]
    pub own_audit_trail: bool,
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// List tools by category or list categories
    List {
        /// Category to filter by
        category: Option<String>,
        /// Show health status
        #[arg(long)]
        health: bool,
    },
    /// Show detailed help for a tool or command
    Help {
        /// Tool name
        tool: String,
        /// Command name (for native tools)
        command: Option<String>,
    },
    /// Run a tool command
    #[command(trailing_var_arg = true)]
    Run {
        /// Tool name
        tool: String,
        /// Command name (native) or tool name (MCP)
        command: String,
        /// Show full output without truncation
        #[arg(long)]
        full: bool,
        /// Timeout in seconds
        #[arg(long)]
        timeout: Option<u64>,
        /// Arguments passed to the tool
        #[arg(allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Show running MCP processes
    Status,
    /// Stop MCP processes
    Stop {
        /// Tool name (stop all if omitted)
        tool: Option<String>,
    },
    /// Validate tool manifests
    Validate {
        /// Specific tool to validate
        tool: Option<String>,
    },
    /// Generate agent system prompt with tool inventory
    AgentPrompt {
        /// Output format: plain or skill
        #[arg(long, default_value = "plain")]
        format: String,
    },
    /// Skill management
    Skill {
        #[command(subcommand)]
        action: SkillAction,
    },
    /// Agent management
    Agent {
        #[command(subcommand)]
        action: AgentAction,
    },
    /// Rule management
    Rule {
        #[command(subcommand)]
        action: RuleAction,
    },
    /// Workflow management
    Workflow {
        #[command(subcommand)]
        action: WorkflowAction,
    },
    /// Audit trail management
    Audit {
        #[command(subcommand)]
        action: AuditAction,
    },
}

#[derive(Subcommand)]
pub enum SkillAction {
    /// List all skills
    List,
    /// Show a skill's content
    Show {
        /// Skill name
        name: String,
    },
    /// Validate skill manifests
    Validate {
        /// Specific skill to validate
        name: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum AgentAction {
    /// List all agents
    List,
    /// Show an agent's prompt
    Show {
        /// Agent name
        name: String,
    },
    /// Validate agent manifests
    Validate {
        /// Specific agent to validate
        name: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum RuleAction {
    /// List all rules
    List,
    /// Show a rule's content
    Show {
        /// Rule name
        name: String,
    },
    /// Validate rule manifests
    Validate {
        /// Specific rule to validate
        name: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum WorkflowAction {
    /// List all workflows
    List,
    /// Show a workflow's steps
    Show {
        /// Workflow name
        name: String,
    },
    /// Validate workflow manifests
    Validate {
        /// Specific workflow to validate
        name: Option<String>,
    },
    /// Run a workflow
    Run {
        /// Workflow name
        name: String,
        /// Show full output without truncation
        #[arg(long)]
        full: bool,
        /// Override workflow timeout (seconds)
        #[arg(long)]
        timeout: Option<u64>,
        /// Print each step's output with headers
        #[arg(long)]
        verbose: bool,
    },
}

#[derive(Subcommand)]
pub enum AuditAction {
    /// List recent audit files
    List {
        /// Maximum number of files to show
        #[arg(long, default_value = "20")]
        limit: usize,
    },
    /// Verify hash chain integrity
    Verify {
        /// Session ID (verify all if omitted)
        session: Option<String>,
    },
    /// Query audit entries
    Query {
        /// Session ID (query all if omitted)
        session: Option<String>,
        /// Filter by event name
        #[arg(long)]
        event: Option<String>,
        /// Filter by outcome
        #[arg(long)]
        outcome: Option<String>,
        /// Only entries after this timestamp (ISO-8601 or YYYY-MM-DD)
        #[arg(long)]
        since: Option<String>,
        /// Free-text search across entry data
        #[arg(long)]
        search: Option<String>,
        /// Maximum number of entries to return
        #[arg(long, default_value = "50")]
        limit: usize,
    },
}
