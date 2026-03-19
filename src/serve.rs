use std::{collections::HashMap, sync::Arc};

use axum::{
    body::Body,
    extract::{Query, State},
    http::{header, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, Mutex, RwLock};
use tokio_util::sync::CancellationToken;

use crate::{
    daemon,
    daemon::state::{DaemonState, Status},
    error::ToolshedError,
    manifest::{ArgType, ToolType},
    mcp,
    mcp::protocol::{ContentItem, McpToolDef, ToolCallResult, MCP_PROTOCOL_VERSION},
    registry::{Registry, Tool},
    runner,
};

// ── Incoming JSON-RPC ──

#[derive(Debug, Deserialize)]
struct IncomingJsonRpc {
    #[serde(rename = "jsonrpc")]
    _jsonrpc: String,
    id: Option<serde_json::Value>,
    method: String,
    params: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
struct OutgoingJsonRpc {
    jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcErrorBody>,
}

#[derive(Debug, Serialize)]
struct JsonRpcErrorBody {
    code: i64,
    message: String,
}

impl OutgoingJsonRpc {
    fn result(id: Option<serde_json::Value>, result: serde_json::Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(result),
            error: None,
        }
    }

    fn error(id: Option<serde_json::Value>, code: i64, message: String) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(JsonRpcErrorBody { code, message }),
        }
    }
}

// ── Exposed tool index ──

#[derive(Debug, Clone)]
struct ExposedTool {
    namespaced_name: String,
    def: McpToolDef,
    tool_name: String,
    command_name: String,
    tool_type: ToolType,
}

struct AppState {
    exposed: Vec<ExposedTool>,
    registry: Arc<Registry>,
    /// Per-session channels: `session_id` to sender for SSE responses
    sessions: Mutex<HashMap<String, mpsc::UnboundedSender<String>>>,
    daemon_state: Arc<RwLock<DaemonState>>,
    cancel: CancellationToken,
}

// ── Build tool index ──

async fn build_tool_index(registry: &Registry, category_filter: Option<&str>) -> Vec<ExposedTool> {
    let mut exposed = Vec::new();

    for (name, tool) in &registry.tools {
        if let Some(cat) = category_filter {
            if tool.manifest.category != cat {
                continue;
            }
        }

        match tool.manifest.tool_type {
            ToolType::Native => {
                for (cmd_name, cmd_def) in &tool.manifest.commands {
                    let namespaced = format!("{name}__{cmd_name}");
                    let schema = build_native_schema(cmd_def);
                    let description = Some(format!(
                        "{} — {}",
                        tool.manifest.description, cmd_def.description
                    ));

                    exposed.push(ExposedTool {
                        namespaced_name: namespaced.clone(),
                        def: McpToolDef {
                            name: namespaced,
                            description,
                            input_schema: Some(schema),
                        },
                        tool_name: name.clone(),
                        command_name: cmd_name.clone(),
                        tool_type: ToolType::Native,
                    });
                }
            }
            ToolType::Mcp => match mcp::introspect::get_raw_mcp_tool_defs(tool).await {
                Ok(defs) => {
                    for mcp_def in defs {
                        let namespaced = format!("{name}__{}", mcp_def.name);
                        let mut schema = mcp_def.input_schema.clone();
                        if let Some(ref mut s) = schema {
                            sanitize_schema(s);
                        }
                        exposed.push(ExposedTool {
                            namespaced_name: namespaced.clone(),
                            def: McpToolDef {
                                name: namespaced,
                                description: mcp_def.description.clone(),
                                input_schema: schema,
                            },
                            tool_name: name.clone(),
                            command_name: mcp_def.name,
                            tool_type: ToolType::Mcp,
                        });
                    }
                }
                #[allow(clippy::print_stderr)]
                Err(e) => {
                    eprintln!("warning: failed to introspect MCP tool '{name}': {e}");
                }
            },
        }
    }

    exposed
}

fn build_native_schema(cmd: &crate::manifest::CommandDef) -> serde_json::Value {
    let mut properties = serde_json::Map::new();
    let mut required = Vec::new();

    for (arg_name, arg_def) in &cmd.args {
        let json_type = match arg_def.arg_type {
            ArgType::String => "string",
            ArgType::Int => "integer",
            ArgType::Float => "number",
            ArgType::Bool => "boolean",
        };

        let mut prop = serde_json::Map::new();
        prop.insert("type".to_string(), serde_json::json!(json_type));
        if let Some(desc) = &arg_def.description {
            prop.insert("description".to_string(), serde_json::json!(desc));
        }
        if let Some(default) = &arg_def.default {
            prop.insert("default".to_string(), default.clone());
        }

        properties.insert(arg_name.clone(), serde_json::Value::Object(prop));

        if arg_def.required {
            required.push(serde_json::Value::String(arg_name.clone()));
        }
    }

    serde_json::json!({
        "type": "object",
        "properties": properties,
        "required": required,
    })
}

// ── Schema sanitization (vLLM compat) ──

/// Recursively sanitize JSON Schema so vLLM's `trim_schema()` won't crash.
/// vLLM expects every property to have a "type" field. Properties using
/// "anyOf" without a "type" key cause a `KeyError` in vLLM's `trim_schema`.
fn sanitize_schema(schema: &mut serde_json::Value) {
    let Some(obj) = schema.as_object_mut() else {
        return;
    };

    // If this object has "anyOf" but no "type", resolve it
    if obj.contains_key("anyOf") && !obj.contains_key("type") {
        if let Some(any_of) = obj.remove("anyOf") {
            if let Some(arr) = any_of.as_array() {
                // Collect non-null types
                let types: Vec<&str> = arr
                    .iter()
                    .filter_map(|v| v.get("type").and_then(|t| t.as_str()))
                    .filter(|t| *t != "null")
                    .collect();
                if types.is_empty() {
                    obj.insert("type".to_string(), serde_json::json!("string"));
                } else {
                    obj.insert("type".to_string(), serde_json::json!(types[0]));
                }
            }
        }
    }

    // Recurse into properties
    if let Some(props) = obj.get_mut("properties") {
        if let Some(props_obj) = props.as_object_mut() {
            for (_, v) in props_obj.iter_mut() {
                sanitize_schema(v);
            }
        }
    }

    // Recurse into items
    if let Some(items) = obj.get_mut("items") {
        sanitize_schema(items);
    }
}

// ── JSON args to CLI args ──

fn json_to_cli_args(
    cmd_def: &crate::manifest::CommandDef,
    arguments: &serde_json::Value,
) -> Vec<String> {
    let Some(map) = arguments.as_object() else {
        return Vec::new();
    };

    let mut positional_args: Vec<(String, String)> = Vec::new();
    let mut flag_args: Vec<(String, String)> = Vec::new();

    for (name, arg_def) in &cmd_def.args {
        if let Some(val) = map.get(name) {
            let stringified = match val {
                serde_json::Value::String(s) => s.clone(),
                serde_json::Value::Bool(b) => b.to_string(),
                serde_json::Value::Number(n) => n.to_string(),
                other => other.to_string(),
            };

            if arg_def.positional {
                positional_args.push((name.clone(), stringified));
            } else {
                flag_args.push((name.clone(), stringified));
            }
        }
    }

    let mut args = Vec::new();
    for (_, val) in positional_args {
        args.push(val);
    }
    for (name, val) in flag_args {
        args.push(format!("--{name}"));
        args.push(val);
    }
    args
}

// ── SSE endpoint: GET /sse ──

async fn handle_sse(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let session_id = uuid::Uuid::new_v4().to_string();
    let (tx, mut rx) = mpsc::unbounded_channel::<String>();

    // Register session
    state.sessions.lock().await.insert(session_id.clone(), tx);

    let sid = session_id.clone();
    let state_clone = state.clone();

    let stream = async_stream::stream! {
        // Send the endpoint event first
        let endpoint_url = format!("/messages?sessionId={sid}");
        yield Ok::<_, std::convert::Infallible>(
            format!("event: endpoint\ndata: {endpoint_url}\n\n")
        );

        // Relay responses from the channel
        while let Some(msg) = rx.recv().await {
            yield Ok(format!("event: message\ndata: {msg}\n\n"));
        }

        // Clean up session on disconnect
        state_clone.sessions.lock().await.remove(&sid);
    };

    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "text/event-stream"),
            (header::CACHE_CONTROL, "no-cache"),
            (header::CONNECTION, "keep-alive"),
        ],
        Body::from_stream(stream),
    )
}

// ── Messages endpoint: POST /messages ──

#[derive(Debug, Deserialize)]
struct MessageQuery {
    #[serde(rename = "sessionId")]
    session_id: String,
}

async fn handle_messages(
    State(state): State<Arc<AppState>>,
    Query(query): Query<MessageQuery>,
    Json(req): Json<IncomingJsonRpc>,
) -> impl IntoResponse {
    let session_id = query.session_id;

    let resp = process_rpc(&state, req).await;

    // Only send responses (not notification acks) through SSE
    if resp.id.is_some() {
        let json_str = serde_json::to_string(&resp).unwrap_or_default();
        let sessions = state.sessions.lock().await;
        if let Some(tx) = sessions.get(&session_id) {
            let _ = tx.send(json_str);
        }
        // Silently drop responses for missing sessions — this can happen
        // when an SSE connection closes before the response is ready.
    }

    StatusCode::ACCEPTED
}

// ── Plain POST endpoint: POST / ──

async fn handle_post(
    State(state): State<Arc<AppState>>,
    Json(req): Json<IncomingJsonRpc>,
) -> Json<OutgoingJsonRpc> {
    Json(process_rpc(&state, req).await)
}

// ── Health endpoint: GET /health ──

async fn handle_health(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let (uptime, any_recovering, tools_json, next_refresh, vault_reachable) =
        snapshot_health(&*state.daemon_state.read().await);

    let overall_status = if any_recovering {
        "recovering"
    } else {
        "healthy"
    };
    let body = serde_json::json!({
        "status": overall_status,
        "uptime_secs": uptime,
        "tools": tools_json,
        "auth": {
            "next_refresh": next_refresh,
            "vault_reachable": vault_reachable,
        }
    });
    let status_code = if any_recovering {
        StatusCode::SERVICE_UNAVAILABLE
    } else {
        StatusCode::OK
    };
    (status_code, Json(body))
}

fn snapshot_health(
    daemon: &DaemonState,
) -> (
    u64,
    bool,
    serde_json::Map<String, serde_json::Value>,
    Option<String>,
    bool,
) {
    let uptime = daemon.uptime_secs();
    let mut any_recovering = false;
    let mut tools_json = serde_json::Map::new();
    for (name, ts) in &daemon.tool_status {
        if ts.status == Status::Recovering {
            any_recovering = true;
        }
        let status_str = match ts.status {
            Status::Up => "up",
            Status::Down => "down",
            Status::Recovering => "recovering",
        };
        let mut tool_obj = serde_json::Map::new();
        tool_obj.insert("status".to_string(), serde_json::json!(status_str));
        tool_obj.insert("recoveries".to_string(), serde_json::json!(ts.recoveries));
        if let Some(ref err) = ts.last_error {
            tool_obj.insert("last_error".to_string(), serde_json::json!(err));
        }
        if ts.status == Status::Recovering {
            if let Some(attempt) = ts.recovering_attempt {
                tool_obj.insert("attempt".to_string(), serde_json::json!(attempt));
            }
        }
        tools_json.insert(name.clone(), serde_json::Value::Object(tool_obj));
    }
    let next_refresh: Option<String> = daemon
        .secrets
        .values()
        .map(|e| e.next_refresh_at)
        .min()
        .map(|inst| {
            let secs = inst
                .saturating_duration_since(std::time::Instant::now())
                .as_secs();
            format!("in {secs}s")
        });
    let vault_reachable = !daemon.secrets.is_empty();
    (
        uptime,
        any_recovering,
        tools_json,
        next_refresh,
        vault_reachable,
    )
}

// ── JSON-RPC dispatch helpers ──

async fn rpc_tools_call(
    state: &AppState,
    id: Option<serde_json::Value>,
    params: Option<serde_json::Value>,
) -> OutgoingJsonRpc {
    let Some(params) = params else {
        return OutgoingJsonRpc::error(id, -32602, "missing params".to_string());
    };

    let tool_name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
    let arguments = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));

    let Some(exposed) = state
        .exposed
        .iter()
        .find(|e| e.namespaced_name == tool_name)
    else {
        return OutgoingJsonRpc::error(id, -32602, format!("unknown tool: {tool_name}"));
    };

    let Some(registry_tool) = state.registry.tools.get(&exposed.tool_name) else {
        return OutgoingJsonRpc::error(
            id,
            -32603,
            format!("tool not found in registry: {}", exposed.tool_name),
        );
    };

    // Check tool health
    {
        let daemon = state.daemon_state.read().await;
        if let Some(ts) = daemon.tool_status.get(&exposed.tool_name) {
            match ts.status {
                Status::Down => {
                    return OutgoingJsonRpc::error(
                        id,
                        -32003,
                        format!(
                            "Tool '{}' is down: {}",
                            exposed.tool_name,
                            ts.last_error.as_deref().unwrap_or("unknown error")
                        ),
                    );
                }
                Status::Recovering => {
                    return OutgoingJsonRpc::error(
                        id,
                        -32003,
                        format!(
                            "Tool '{}' is temporarily unavailable (recovering, attempt {})",
                            exposed.tool_name,
                            ts.recovering_attempt.unwrap_or(0)
                        ),
                    );
                }
                Status::Up => {}
            }
        }
    }

    let daemon = state.daemon_state.read().await;
    let result = dispatch_tool(registry_tool, exposed, &arguments, Some(&daemon)).await;
    drop(daemon);

    let (call_result, is_error) = match result {
        Ok(text) => (text, false),
        Err(e) => (e.to_string(), true),
    };

    let tool_call_result = ToolCallResult {
        content: vec![ContentItem::Text { text: call_result }],
        is_error,
    };

    match serde_json::to_value(tool_call_result) {
        Ok(v) => OutgoingJsonRpc::result(id, v),
        Err(e) => OutgoingJsonRpc::error(id, -32603, format!("serialization failed: {e}")),
    }
}

async fn rpc_daemon_health(state: &AppState, id: Option<serde_json::Value>) -> OutgoingJsonRpc {
    let (uptime, any_recovering, tools_json) = {
        let daemon = state.daemon_state.read().await;
        let uptime = daemon.uptime_secs();
        let mut tools_json = serde_json::Map::new();
        let mut any_recovering = false;
        for (name, ts) in &daemon.tool_status {
            if ts.status == Status::Recovering {
                any_recovering = true;
            }
            let status_str = match ts.status {
                Status::Up => "up",
                Status::Down => "down",
                Status::Recovering => "recovering",
            };
            let mut obj = serde_json::Map::new();
            obj.insert("status".to_string(), serde_json::json!(status_str));
            obj.insert("recoveries".to_string(), serde_json::json!(ts.recoveries));
            tools_json.insert(name.clone(), serde_json::Value::Object(obj));
        }
        drop(daemon);
        (uptime, any_recovering, tools_json)
    };

    let status = if any_recovering {
        "recovering"
    } else {
        "healthy"
    };
    OutgoingJsonRpc::result(
        id,
        serde_json::json!({
            "status": status,
            "uptime_secs": uptime,
            "tools": tools_json,
        }),
    )
}

// ── JSON-RPC dispatch ──

async fn process_rpc(state: &AppState, req: IncomingJsonRpc) -> OutgoingJsonRpc {
    let id = req.id.clone();

    match req.method.as_str() {
        "initialize" => OutgoingJsonRpc::result(
            id,
            serde_json::json!({
                "protocolVersion": MCP_PROTOCOL_VERSION,
                "capabilities": {
                    "tools": {}
                },
                "serverInfo": {
                    "name": "toolshed",
                    "version": env!("CARGO_PKG_VERSION")
                }
            }),
        ),

        "notifications/initialized" => {
            // Notification — no response needed, but we send one anyway
            // since the client might be waiting
            OutgoingJsonRpc::result(id, serde_json::json!({}))
        }

        "tools/list" => {
            let tools: Vec<&McpToolDef> = state.exposed.iter().map(|e| &e.def).collect();
            OutgoingJsonRpc::result(
                id,
                serde_json::json!({
                    "tools": tools,
                }),
            )
        }

        "tools/call" => rpc_tools_call(state, id, req.params).await,

        "daemon/health" => rpc_daemon_health(state, id).await,

        _ => OutgoingJsonRpc::error(id, -32601, format!("method not found: {}", req.method)),
    }
}

async fn dispatch_tool(
    tool: &Tool,
    exposed: &ExposedTool,
    arguments: &serde_json::Value,
    daemon_state: Option<&DaemonState>,
) -> Result<String, ToolshedError> {
    match exposed.tool_type {
        ToolType::Native => {
            let cmd_def = tool
                .manifest
                .commands
                .get(&exposed.command_name)
                .ok_or_else(|| ToolshedError::CommandNotFound {
                    tool: exposed.tool_name.clone(),
                    command: exposed.command_name.clone(),
                })?;
            let cli_args = json_to_cli_args(cmd_def, arguments);
            runner::native::run(tool, &exposed.command_name, &cli_args, None).await
        }
        ToolType::Mcp => {
            let mcp_cfg =
                tool.manifest
                    .mcp
                    .as_ref()
                    .ok_or_else(|| ToolshedError::MissingMcpConfig {
                        tool: exposed.tool_name.clone(),
                    })?;
            match mcp_cfg.transport {
                crate::manifest::McpTransport::Stdio => {
                    mcp::stdio::call_tool_with_state(
                        tool,
                        &exposed.command_name,
                        arguments.clone(),
                        None,
                        daemon_state,
                    )
                    .await
                }
                crate::manifest::McpTransport::Http => {
                    mcp::http::call_tool_with_state(
                        tool,
                        &exposed.command_name,
                        arguments.clone(),
                        None,
                        daemon_state,
                    )
                    .await
                }
            }
        }
    }
}

// ── Server entry point ──

#[allow(clippy::print_stderr)]
pub async fn serve(port: u16, category: Option<String>) -> Result<(), ToolshedError> {
    let registry = Registry::load()?;
    let registry = Arc::new(registry);

    eprintln!("indexing tools...");
    let exposed = build_tool_index(&registry, category.as_deref()).await;

    eprintln!("indexed {} tools:", exposed.len());
    for tool in &exposed {
        eprintln!("  {}", tool.namespaced_name);
    }

    // Parse vault-env.sh for secret definitions
    let vault_env_path = crate::config::toolshed_dir().join("vault-env.sh");
    let secret_defs = if vault_env_path.exists() {
        let script = std::fs::read_to_string(&vault_env_path).map_err(ToolshedError::Io)?;
        daemon::auth::parse_vault_env_script(&script)
    } else {
        Vec::new()
    };

    let vault_addr = std::env::var("VAULT_ADDR").ok();
    let vault_token = std::env::var("VAULT_TOKEN").ok();

    // Spawn daemon
    let (cancel, daemon_state) =
        daemon::spawn_daemon(registry.clone(), secret_defs, vault_addr, vault_token).await;

    let state = Arc::new(AppState {
        exposed,
        registry,
        sessions: Mutex::new(HashMap::new()),
        daemon_state,
        cancel: cancel.clone(),
    });

    let app = Router::new()
        .route("/sse", get(handle_sse))
        .route("/messages", post(handle_messages))
        .route("/health", get(handle_health))
        .route("/", post(handle_post))
        .with_state(state);

    let addr = format!("0.0.0.0:{port}");
    eprintln!("listening on {addr}");

    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .map_err(ToolshedError::Io)?;

    // Graceful shutdown on SIGINT (Ctrl-C) or SIGTERM (Docker stop)
    let shutdown_cancel = cancel.clone();
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            let ctrl_c = tokio::signal::ctrl_c();
            #[cfg(unix)]
            let terminate = async {
                if let Ok(mut sig) =
                    tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                {
                    sig.recv().await;
                }
            };
            #[cfg(not(unix))]
            let terminate = std::future::pending::<()>();

            tokio::select! {
                _ = ctrl_c => {},
                () = terminate => {},
            }
            eprintln!("daemon: received shutdown signal");
            shutdown_cancel.cancel();
        })
        .await
        .map_err(ToolshedError::Io)?;

    Ok(())
}

// ── Tests ──

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    #[test]
    fn test_build_native_schema() {
        let mut args = BTreeMap::new();
        args.insert(
            "query".to_string(),
            crate::manifest::ArgDef {
                arg_type: ArgType::String,
                required: true,
                positional: true,
                default: None,
                description: Some("Search query".to_string()),
            },
        );
        args.insert(
            "limit".to_string(),
            crate::manifest::ArgDef {
                arg_type: ArgType::Int,
                required: false,
                positional: false,
                default: Some(serde_json::json!(10)),
                description: Some("Max results".to_string()),
            },
        );

        let cmd = crate::manifest::CommandDef {
            description: "test".to_string(),
            args,
        };

        let schema = build_native_schema(&cmd);
        let props = schema.get("properties").unwrap().as_object().unwrap();
        assert_eq!(props["query"]["type"], "string");
        assert_eq!(props["limit"]["type"], "integer");
        assert_eq!(props["limit"]["default"], 10);

        let required = schema.get("required").unwrap().as_array().unwrap();
        assert!(required.contains(&serde_json::json!("query")));
        assert!(!required.contains(&serde_json::json!("limit")));
    }

    #[test]
    fn test_json_to_cli_args() {
        let mut args = BTreeMap::new();
        args.insert(
            "file".to_string(),
            crate::manifest::ArgDef {
                arg_type: ArgType::String,
                required: true,
                positional: true,
                default: None,
                description: None,
            },
        );
        args.insert(
            "verbose".to_string(),
            crate::manifest::ArgDef {
                arg_type: ArgType::Bool,
                required: false,
                positional: false,
                default: None,
                description: None,
            },
        );

        let cmd = crate::manifest::CommandDef {
            description: "test".to_string(),
            args,
        };

        let arguments = serde_json::json!({
            "file": "test.txt",
            "verbose": true
        });

        let cli_args = json_to_cli_args(&cmd, &arguments);
        assert_eq!(cli_args[0], "test.txt");
        assert!(cli_args.contains(&"--verbose".to_string()));
        assert!(cli_args.contains(&"true".to_string()));
    }

    #[test]
    fn test_outgoing_jsonrpc_result() {
        let resp =
            OutgoingJsonRpc::result(Some(serde_json::json!(1)), serde_json::json!({"ok": true}));
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"result\""));
        assert!(!json.contains("\"error\""));
    }

    #[test]
    fn test_outgoing_jsonrpc_error() {
        let resp =
            OutgoingJsonRpc::error(Some(serde_json::json!(1)), -32601, "not found".to_string());
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"error\""));
        assert!(!json.contains("\"result\""));
    }

    #[test]
    fn test_down_tool_error_message() {
        let resp = OutgoingJsonRpc::error(
            Some(serde_json::json!(1)),
            -32003,
            "Tool 'sourcegraph' is down: connection refused".to_string(),
        );
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("-32003"));
        assert!(json.contains("sourcegraph"));
        assert!(json.contains("connection refused"));
    }

    #[test]
    fn test_recovering_tool_error_message() {
        let resp = OutgoingJsonRpc::error(
            Some(serde_json::json!(1)),
            -32003,
            "Tool 'vault' is temporarily unavailable (recovering, attempt 2)".to_string(),
        );
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("recovering"));
        assert!(json.contains("attempt 2"));
    }

    #[test]
    fn test_health_response_serialization() {
        let resp = serde_json::json!({
            "status": "healthy",
            "uptime_secs": 84720,
            "tools": {
                "code-search": { "status": "up", "last_check": "2026-03-18T14:30:00Z", "recoveries": 0 },
            },
            "auth": {
                "next_refresh": "2026-03-18T15:18:00Z",
                "vault_reachable": true,
            }
        });
        let json_str = serde_json::to_string(&resp).unwrap();
        assert!(json_str.contains("healthy"));
        assert!(json_str.contains("code-search"));
    }
}
