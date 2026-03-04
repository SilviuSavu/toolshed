use crate::config;
use crate::error::ToolshedError;
use crate::manifest::McpTransport;
use crate::mcp;
use crate::mcp::protocol::McpToolDef;
use crate::registry::Tool;
use std::path::PathBuf;
use std::time::SystemTime;

/// Parsed parameter info for display.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ParamInfo {
    pub name: String,
    pub param_type: String,
    pub required: bool,
    pub description: Option<String>,
}

/// Friendly tool info for help display.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct McpToolInfo {
    pub name: String,
    pub description: Option<String>,
    pub params: Vec<ParamInfo>,
}

impl McpToolInfo {
    pub fn format_params(&self) -> String {
        self.params
            .iter()
            .map(|p| {
                if p.required {
                    format!("{}: {}", p.name, p.param_type)
                } else {
                    format!("{}?: {}", p.name, p.param_type)
                }
            })
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// Get MCP tools, using cache if available and fresh.
pub async fn get_mcp_tools(tool: &Tool) -> Result<Vec<McpToolInfo>, ToolshedError> {
    let cache_path = cache_path_for(tool);

    // Check cache
    if let Some(cached) = read_cache(&cache_path) {
        return Ok(cached);
    }

    // Fetch from server
    let mcp_cfg = tool.manifest.mcp.as_ref().unwrap();
    let tool_defs = match mcp_cfg.transport {
        McpTransport::Stdio => mcp::stdio::list_tools(tool).await?,
        McpTransport::Http => mcp::http::list_tools(tool).await?,
    };

    let infos: Vec<McpToolInfo> = tool_defs.iter().map(convert_tool_def).collect();

    // Write cache
    write_cache(&cache_path, &infos);

    Ok(infos)
}

fn convert_tool_def(def: &McpToolDef) -> McpToolInfo {
    let mut params = Vec::new();

    if let Some(schema) = &def.input_schema {
        if let Some(properties) = schema.get("properties").and_then(|p| p.as_object()) {
            let required_list: Vec<String> = schema
                .get("required")
                .and_then(|r| r.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default();

            for (name, prop) in properties {
                let param_type = prop
                    .get("type")
                    .and_then(|t| t.as_str())
                    .unwrap_or("any")
                    .to_string();
                let description = prop
                    .get("description")
                    .and_then(|d| d.as_str())
                    .map(|s| s.to_string());
                let required = required_list.contains(name);

                params.push(ParamInfo {
                    name: name.clone(),
                    param_type,
                    required,
                    description,
                });
            }
        }
    }

    McpToolInfo {
        name: def.name.clone(),
        description: def.description.clone(),
        params,
    }
}

fn cache_path_for(tool: &Tool) -> PathBuf {
    config::cache_dir().join(format!("{}.tools.json", tool.manifest.name))
}

fn read_cache(path: &PathBuf) -> Option<Vec<McpToolInfo>> {
    let metadata = std::fs::metadata(path).ok()?;
    let modified = metadata.modified().ok()?;
    let age = SystemTime::now().duration_since(modified).ok()?;

    if age.as_secs() > config::INTROSPECT_CACHE_TTL_SECS {
        return None;
    }

    let content = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

fn write_cache(path: &PathBuf, tools: &[McpToolInfo]) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string_pretty(tools) {
        let _ = std::fs::write(path, json);
    }
}
