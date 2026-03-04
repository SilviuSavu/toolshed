use crate::error::ToolshedError;
use crate::manifest::McpTransport;
use crate::mcp;
use crate::registry::Tool;

pub async fn run(
    tool: &Tool,
    command: &str,
    args: &[String],
    timeout: Option<u64>,
) -> Result<String, ToolshedError> {
    let mcp_cfg = tool
        .manifest
        .mcp
        .as_ref()
        .ok_or_else(|| ToolshedError::MissingMcpConfig {
            tool: tool.manifest.name.clone(),
        })?;

    // Parse --key value pairs into JSON object
    let arguments = parse_mcp_args(args)?;

    match mcp_cfg.transport {
        McpTransport::Stdio => {
            mcp::stdio::call_tool(tool, command, arguments, timeout).await
        }
        McpTransport::Http => {
            mcp::http::call_tool(tool, command, arguments, timeout).await
        }
    }
}

fn parse_mcp_args(args: &[String]) -> Result<serde_json::Value, ToolshedError> {
    let mut map = serde_json::Map::new();
    let mut i = 0;

    while i < args.len() {
        let arg = &args[i];
        if let Some(key) = arg.strip_prefix("--") {
            if i + 1 < args.len() {
                let val = &args[i + 1];
                // Try to parse as JSON value (number, bool, etc.), fall back to string
                let json_val = serde_json::from_str(val)
                    .unwrap_or_else(|_| serde_json::Value::String(val.clone()));
                map.insert(key.to_string(), json_val);
                i += 2;
            } else {
                // Flag without value — treat as true
                map.insert(key.to_string(), serde_json::Value::Bool(true));
                i += 1;
            }
        } else {
            // Positional args not supported for MCP
            i += 1;
        }
    }

    Ok(serde_json::Value::Object(map))
}
