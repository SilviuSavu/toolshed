use crate::error::ToolshedError;
use crate::manifest::{ArgType, ToolType};
use crate::mcp::protocol::{
    ContentItem, McpToolDef, ToolCallResult, MCP_PROTOCOL_VERSION,
};
use crate::registry::{Registry, Tool};
use crate::{mcp, runner};
use axum::body::Body;
use axum::extract::{Query, State};
use axum::http::{header, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};

// ─── Incoming JSON-RPC (flexible id for server use) ─────────

#[derive(Debug, Deserialize)]
struct IncomingJsonRpc {
    #[allow(dead_code)]
    jsonrpc: String,
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

// ─── Exposed tool index ─────────────────────────────────────

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
    registry: Registry,
    /// Per-session channels: session_id → sender for SSE responses
    sessions: Mutex<HashMap<String, mpsc::UnboundedSender<String>>>,
}

// ─── Build tool index ───────────────────────────────────────

async fn build_tool_index(
    registry: &Registry,
    category_filter: Option<&str>,
) -> Vec<ExposedTool> {
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
            ToolType::Mcp => {
                match mcp::introspect::get_raw_mcp_tool_defs(tool).await {
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
                    Err(e) => {
                        eprintln!(
                            "warning: failed to introspect MCP tool '{}': {}",
                            name, e
                        );
                    }
                }
            }
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

// ─── Schema sanitization (vLLM compat) ─────────────────────

/// Recursively sanitize JSON Schema so vLLM's trim_schema() won't crash.
/// vLLM expects every property to have a "type" field. Properties using
/// "anyOf" without a "type" key cause a KeyError in vLLM's trim_schema.
fn sanitize_schema(schema: &mut serde_json::Value) {
    let obj = match schema.as_object_mut() {
        Some(o) => o,
        None => return,
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
                if types.len() == 1 {
                    obj.insert("type".to_string(), serde_json::json!(types[0]));
                } else if !types.is_empty() {
                    obj.insert("type".to_string(), serde_json::json!(types[0]));
                } else {
                    obj.insert("type".to_string(), serde_json::json!("string"));
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

// ─── JSON args → CLI args ───────────────────────────────────

fn json_to_cli_args(
    cmd_def: &crate::manifest::CommandDef,
    arguments: &serde_json::Value,
) -> Vec<String> {
    let map = match arguments.as_object() {
        Some(m) => m,
        None => return Vec::new(),
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

// ─── SSE endpoint: GET /sse ─────────────────────────────────

async fn handle_sse(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let session_id = uuid::Uuid::new_v4().to_string();
    let (tx, mut rx) = mpsc::unbounded_channel::<String>();

    // Register session
    state
        .sessions
        .lock()
        .await
        .insert(session_id.clone(), tx);

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

// ─── Messages endpoint: POST /messages ──────────────────────

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
    let id = req.id.clone();

    let resp = process_rpc(&state, req).await;

    // Only send responses (not notification acks) through SSE
    if resp.id.is_some() {
        let json_str = serde_json::to_string(&resp).unwrap_or_default();
        let sessions = state.sessions.lock().await;
        if let Some(tx) = sessions.get(&session_id) {
            let _ = tx.send(json_str);
        } else {
            eprintln!(
                "warning: no SSE session for id={}, method response for id={:?} dropped",
                session_id, id
            );
        }
    }

    StatusCode::ACCEPTED
}

// ─── Plain POST endpoint: POST / ────────────────────────────

async fn handle_post(
    State(state): State<Arc<AppState>>,
    Json(req): Json<IncomingJsonRpc>,
) -> Json<OutgoingJsonRpc> {
    Json(process_rpc(&state, req).await)
}

async fn process_rpc(state: &AppState, req: IncomingJsonRpc) -> OutgoingJsonRpc {
    let id = req.id.clone();

    match req.method.as_str() {
        "initialize" => {
            OutgoingJsonRpc::result(
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
            )
        }

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

        "tools/call" => {
            let params = match req.params {
                Some(p) => p,
                None => {
                    return OutgoingJsonRpc::error(id, -32602, "missing params".to_string());
                }
            };

            let tool_name = params
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let arguments = params
                .get("arguments")
                .cloned()
                .unwrap_or(serde_json::json!({}));

            let exposed = match state
                .exposed
                .iter()
                .find(|e| e.namespaced_name == tool_name)
            {
                Some(e) => e,
                None => {
                    return OutgoingJsonRpc::error(
                        id,
                        -32602,
                        format!("unknown tool: {tool_name}"),
                    );
                }
            };

            let registry_tool = match state.registry.tools.get(&exposed.tool_name) {
                Some(t) => t,
                None => {
                    return OutgoingJsonRpc::error(
                        id,
                        -32603,
                        format!("tool not found in registry: {}", exposed.tool_name),
                    );
                }
            };

            let result = dispatch_tool(registry_tool, exposed, &arguments).await;

            let (call_result, is_error) = match result {
                Ok(text) => (text, false),
                Err(e) => (e.to_string(), true),
            };

            let tool_call_result = ToolCallResult {
                content: vec![ContentItem::Text { text: call_result }],
                is_error,
            };

            OutgoingJsonRpc::result(id, serde_json::to_value(tool_call_result).unwrap())
        }

        _ => OutgoingJsonRpc::error(id, -32601, format!("method not found: {}", req.method)),
    }
}

async fn dispatch_tool(
    tool: &Tool,
    exposed: &ExposedTool,
    arguments: &serde_json::Value,
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
                    mcp::stdio::call_tool(tool, &exposed.command_name, arguments.clone(), None)
                        .await
                }
                crate::manifest::McpTransport::Http => {
                    mcp::http::call_tool(tool, &exposed.command_name, arguments.clone(), None)
                        .await
                }
            }
        }
    }
}

// ─── Server entry point ─────────────────────────────────────

pub async fn serve(port: u16, category: Option<String>) -> Result<(), ToolshedError> {
    let registry = Registry::load()?;

    eprintln!("indexing tools...");
    let exposed = build_tool_index(&registry, category.as_deref()).await;

    eprintln!("indexed {} tools:", exposed.len());
    for tool in &exposed {
        eprintln!("  {}", tool.namespaced_name);
    }

    let state = Arc::new(AppState {
        exposed,
        registry,
        sessions: Mutex::new(HashMap::new()),
    });

    let app = Router::new()
        .route("/sse", get(handle_sse))
        .route("/messages", post(handle_messages))
        .route("/", post(handle_post))
        .with_state(state);

    let addr = format!("0.0.0.0:{port}");
    eprintln!("listening on {addr}");

    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .map_err(ToolshedError::Io)?;

    axum::serve(listener, app)
        .await
        .map_err(|e| ToolshedError::Io(e.into()))?;

    Ok(())
}

// ─── Tests ──────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

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
}
