use clap::{Arg, ArgMatches};

pub struct Cli {
    pub own_audit_trail: bool,
    pub command: Command,
}

impl Cli {
    pub fn parse() -> Self {
        let matches = build_cli().get_matches();
        parse_matches(&matches)
    }
}

pub enum Command {
    List {
        category: Option<String>,
        health: bool,
    },
    Help {
        tool: String,
        command: Option<String>,
    },
    Run {
        tool: String,
        command: String,
        full: bool,
        timeout: Option<u64>,
        args: Vec<String>,
    },
    Status,
    Stop {
        tool: Option<String>,
    },
    Validate {
        tool: Option<String>,
    },
    AgentPrompt {
        format: String,
    },
    Skill {
        action: SkillAction,
    },
    Agent {
        action: AgentAction,
    },
    Rule {
        action: RuleAction,
    },
    Workflow {
        action: WorkflowAction,
    },
    Serve {
        port: u16,
        category: Option<String>,
    },
    Audit {
        action: AuditAction,
    },
}

pub enum SkillAction {
    List,
    Show { name: String },
    Validate { name: Option<String> },
}

pub enum AgentAction {
    List,
    Show { name: String },
    Validate { name: Option<String> },
}

pub enum RuleAction {
    List,
    Show { name: String },
    Validate { name: Option<String> },
}

pub enum WorkflowAction {
    List,
    Show {
        name: String,
    },
    Validate {
        name: Option<String>,
    },
    Run {
        name: String,
        full: bool,
        timeout: Option<u64>,
        verbose: bool,
    },
}

pub enum AuditAction {
    List {
        limit: usize,
    },
    Verify {
        session: Option<String>,
    },
    Query {
        session: Option<String>,
        event: Option<String>,
        outcome: Option<String>,
        since: Option<String>,
        search: Option<String>,
        limit: usize,
    },
}

fn resource_subcommand(resource: &'static str) -> clap::Command {
    clap::Command::new(resource)
        .about(format!("{resource} management"))
        .subcommand_required(true)
        .subcommand(clap::Command::new("list").about(format!("List all {resource}s")))
        .subcommand(
            clap::Command::new("show")
                .about(format!("Show a {resource}"))
                .arg(Arg::new("name").required(true)),
        )
        .subcommand(
            clap::Command::new("validate")
                .about(format!("Validate {resource} manifests"))
                .arg(Arg::new("name")),
        )
}

fn workflow_subcommand() -> clap::Command {
    clap::Command::new("workflow")
        .about("Workflow management")
        .subcommand_required(true)
        .subcommand(clap::Command::new("list").about("List all workflows"))
        .subcommand(
            clap::Command::new("show")
                .about("Show a workflow's steps")
                .arg(Arg::new("name").required(true)),
        )
        .subcommand(
            clap::Command::new("validate")
                .about("Validate workflow manifests")
                .arg(Arg::new("name")),
        )
        .subcommand(
            clap::Command::new("run")
                .about("Run a workflow")
                .arg(Arg::new("name").required(true))
                .arg(
                    Arg::new("full")
                        .long("full")
                        .action(clap::ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("timeout")
                        .long("timeout")
                        .value_parser(clap::value_parser!(u64)),
                )
                .arg(
                    Arg::new("verbose")
                        .long("verbose")
                        .action(clap::ArgAction::SetTrue),
                ),
        )
}

fn audit_subcommand() -> clap::Command {
    clap::Command::new("audit")
        .about("Audit trail management")
        .subcommand_required(true)
        .subcommand(
            clap::Command::new("list")
                .about("List recent audit files")
                .arg(
                    Arg::new("limit")
                        .long("limit")
                        .default_value("20")
                        .value_parser(clap::value_parser!(usize)),
                ),
        )
        .subcommand(
            clap::Command::new("verify")
                .about("Verify hash chain integrity")
                .arg(Arg::new("session")),
        )
        .subcommand(
            clap::Command::new("query")
                .about("Query audit entries")
                .arg(Arg::new("session"))
                .arg(Arg::new("event").long("event"))
                .arg(Arg::new("outcome").long("outcome"))
                .arg(Arg::new("since").long("since"))
                .arg(Arg::new("search").long("search"))
                .arg(
                    Arg::new("limit")
                        .long("limit")
                        .default_value("50")
                        .value_parser(clap::value_parser!(usize)),
                ),
        )
}

pub fn build_cli() -> clap::Command {
    clap::Command::new("toolshed")
        .about("Universal tool registry & executor")
        .disable_help_subcommand(true)
        .arg(
            Arg::new("own-audit-trail")
                .long("own-audit-trail")
                .global(true)
                .action(clap::ArgAction::SetTrue),
        )
        .subcommand_required(true)
        .subcommand(
            clap::Command::new("list")
                .about("List tools by category or list categories")
                .arg(Arg::new("category"))
                .arg(
                    Arg::new("health")
                        .long("health")
                        .action(clap::ArgAction::SetTrue),
                ),
        )
        .subcommand(
            clap::Command::new("help")
                .about("Show detailed help for a tool or command")
                .arg(Arg::new("tool").required(true))
                .arg(Arg::new("command")),
        )
        .subcommand(
            clap::Command::new("run")
                .about("Run a tool command")
                .trailing_var_arg(true)
                .arg(Arg::new("tool").required(true))
                .arg(Arg::new("command").required(true))
                .arg(
                    Arg::new("full")
                        .long("full")
                        .action(clap::ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("timeout")
                        .long("timeout")
                        .value_parser(clap::value_parser!(u64)),
                )
                .arg(Arg::new("args").num_args(..).allow_hyphen_values(true)),
        )
        .subcommand(clap::Command::new("status").about("Show running MCP processes"))
        .subcommand(
            clap::Command::new("stop")
                .about("Stop MCP processes")
                .arg(Arg::new("tool")),
        )
        .subcommand(
            clap::Command::new("validate")
                .about("Validate tool manifests")
                .arg(Arg::new("tool")),
        )
        .subcommand(
            clap::Command::new("agent-prompt")
                .about("Generate agent system prompt with tool inventory")
                .arg(Arg::new("format").long("format").default_value("plain")),
        )
        .subcommand(resource_subcommand("skill"))
        .subcommand(resource_subcommand("agent"))
        .subcommand(resource_subcommand("rule"))
        .subcommand(workflow_subcommand())
        .subcommand(
            clap::Command::new("serve")
                .about("Start MCP HTTP server exposing all tools")
                .arg(
                    Arg::new("port")
                        .long("port")
                        .default_value("8888")
                        .value_parser(clap::value_parser!(u16)),
                )
                .arg(Arg::new("category").long("category")),
        )
        .subcommand(audit_subcommand())
}

fn parse_matches(matches: &ArgMatches) -> Cli {
    let own_audit_trail = matches.get_flag("own-audit-trail");
    let (sub, sub_m) = matches.subcommand().unwrap_or(("", matches));

    let command = match sub {
        "list" => Command::List {
            category: sub_m.get_one::<String>("category").cloned(),
            health: sub_m.get_flag("health"),
        },
        "help" => Command::Help {
            tool: sub_m.get_one::<String>("tool").cloned().unwrap_or_default(),
            command: sub_m.get_one::<String>("command").cloned(),
        },
        "run" => Command::Run {
            tool: sub_m.get_one::<String>("tool").cloned().unwrap_or_default(),
            command: sub_m
                .get_one::<String>("command")
                .cloned()
                .unwrap_or_default(),
            full: sub_m.get_flag("full"),
            timeout: sub_m.get_one::<u64>("timeout").copied(),
            args: sub_m
                .get_many::<String>("args")
                .map_or_else(Vec::new, |v| v.cloned().collect()),
        },
        "status" => Command::Status,
        "stop" => Command::Stop {
            tool: sub_m.get_one::<String>("tool").cloned(),
        },
        "validate" => Command::Validate {
            tool: sub_m.get_one::<String>("tool").cloned(),
        },
        "agent-prompt" => Command::AgentPrompt {
            format: sub_m
                .get_one::<String>("format")
                .cloned()
                .unwrap_or_else(|| "plain".into()),
        },
        "skill" => Command::Skill {
            action: parse_skill_action(sub_m),
        },
        "agent" => Command::Agent {
            action: parse_agent_action(sub_m),
        },
        "rule" => Command::Rule {
            action: parse_rule_action(sub_m),
        },
        "workflow" => Command::Workflow {
            action: parse_workflow_action(sub_m),
        },
        "serve" => Command::Serve {
            port: sub_m.get_one::<u16>("port").copied().unwrap_or(8888),
            category: sub_m.get_one::<String>("category").cloned(),
        },
        "audit" => Command::Audit {
            action: parse_audit_action(sub_m),
        },
        _ => unreachable!(),
    };

    Cli {
        own_audit_trail,
        command,
    }
}

fn parse_skill_action(m: &ArgMatches) -> SkillAction {
    let (sub, sub_m) = m.subcommand().unwrap_or(("", m));
    match sub {
        "show" => SkillAction::Show {
            name: sub_m.get_one::<String>("name").cloned().unwrap_or_default(),
        },
        "validate" => SkillAction::Validate {
            name: sub_m.get_one::<String>("name").cloned(),
        },
        _ => SkillAction::List,
    }
}

fn parse_agent_action(m: &ArgMatches) -> AgentAction {
    let (sub, sub_m) = m.subcommand().unwrap_or(("", m));
    match sub {
        "show" => AgentAction::Show {
            name: sub_m.get_one::<String>("name").cloned().unwrap_or_default(),
        },
        "validate" => AgentAction::Validate {
            name: sub_m.get_one::<String>("name").cloned(),
        },
        _ => AgentAction::List,
    }
}

fn parse_rule_action(m: &ArgMatches) -> RuleAction {
    let (sub, sub_m) = m.subcommand().unwrap_or(("", m));
    match sub {
        "show" => RuleAction::Show {
            name: sub_m.get_one::<String>("name").cloned().unwrap_or_default(),
        },
        "validate" => RuleAction::Validate {
            name: sub_m.get_one::<String>("name").cloned(),
        },
        _ => RuleAction::List,
    }
}

fn parse_workflow_action(m: &ArgMatches) -> WorkflowAction {
    let (sub, sub_m) = m.subcommand().unwrap_or(("", m));
    match sub {
        "show" => WorkflowAction::Show {
            name: sub_m.get_one::<String>("name").cloned().unwrap_or_default(),
        },
        "validate" => WorkflowAction::Validate {
            name: sub_m.get_one::<String>("name").cloned(),
        },
        "run" => WorkflowAction::Run {
            name: sub_m.get_one::<String>("name").cloned().unwrap_or_default(),
            full: sub_m.get_flag("full"),
            timeout: sub_m.get_one::<u64>("timeout").copied(),
            verbose: sub_m.get_flag("verbose"),
        },
        _ => WorkflowAction::List,
    }
}

fn parse_audit_action(m: &ArgMatches) -> AuditAction {
    let (sub, sub_m) = m.subcommand().unwrap_or(("", m));
    match sub {
        "verify" => AuditAction::Verify {
            session: sub_m.get_one::<String>("session").cloned(),
        },
        "query" => AuditAction::Query {
            session: sub_m.get_one::<String>("session").cloned(),
            event: sub_m.get_one::<String>("event").cloned(),
            outcome: sub_m.get_one::<String>("outcome").cloned(),
            since: sub_m.get_one::<String>("since").cloned(),
            search: sub_m.get_one::<String>("search").cloned(),
            limit: sub_m.get_one::<usize>("limit").copied().unwrap_or(50),
        },
        _ => AuditAction::List {
            limit: sub_m.get_one::<usize>("limit").copied().unwrap_or(20),
        },
    }
}
