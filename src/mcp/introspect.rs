use std::{
    path::{Path, PathBuf},
    time::SystemTime,
};

use crate::{
    config, error::ToolshedError, manifest::McpTransport, mcp, mcp::protocol::McpToolDef,
    registry::Tool,
};

/// Parsed parameter info for display.
#[derive(Debug, Clone)]
pub struct ParamInfo {
    pub name: String,
    pub param_type: String,
    pub required: bool,
    pub description: Option<String>,
}

impl serde::Serialize for ParamInfo {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let count = 3 + usize::from(self.description.is_some());
        let mut map = serializer.serialize_map(Some(count))?;
        map.serialize_entry("name", &self.name)?;
        map.serialize_entry("param_type", &self.param_type)?;
        map.serialize_entry("required", &self.required)?;
        if let Some(ref d) = self.description { map.serialize_entry("description", d)?; }
        map.end()
    }
}

impl<'de> serde::Deserialize<'de> for ParamInfo {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let v = serde_json::Value::deserialize(deserializer).map_err(serde::de::Error::custom)?;
        let obj = v.as_object().ok_or_else(|| serde::de::Error::custom("expected object"))?;
        Ok(Self {
            name: obj.get("name").and_then(serde_json::Value::as_str).unwrap_or_default().to_string(),
            param_type: obj.get("param_type").and_then(serde_json::Value::as_str).unwrap_or("any").to_string(),
            required: obj.get("required").and_then(serde_json::Value::as_bool).unwrap_or_default(),
            description: obj.get("description").and_then(serde_json::Value::as_str).map(String::from),
        })
    }
}

/// Friendly tool info for help display.
#[derive(Debug, Clone)]
pub struct McpToolInfo {
    pub name: String,
    pub description: Option<String>,
    pub params: Vec<ParamInfo>,
}

impl serde::Serialize for McpToolInfo {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let count = 2 + usize::from(self.description.is_some());
        let mut map = serializer.serialize_map(Some(count))?;
        map.serialize_entry("name", &self.name)?;
        if let Some(ref d) = self.description { map.serialize_entry("description", d)?; }
        map.serialize_entry("params", &self.params)?;
        map.end()
    }
}

impl<'de> serde::Deserialize<'de> for McpToolInfo {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let v = serde_json::Value::deserialize(deserializer).map_err(serde::de::Error::custom)?;
        let obj = v.as_object().ok_or_else(|| serde::de::Error::custom("expected object"))?;
        Ok(Self {
            name: obj.get("name").and_then(serde_json::Value::as_str).unwrap_or_default().to_string(),
            description: obj.get("description").and_then(serde_json::Value::as_str).map(String::from),
            params: obj.get("params").map_or_else(|| Ok(Vec::new()), |v| {
                Vec::<ParamInfo>::deserialize(v).map_err(serde::de::Error::custom)
            })?,
        })
    }
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
    let mcp_cfg = tool
        .manifest
        .mcp
        .as_ref()
        .ok_or_else(|| ToolshedError::MissingMcpConfig {
            tool: tool.manifest.name.clone(),
        })?;
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
                        .filter_map(|v| v.as_str().map(std::string::ToString::to_string))
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
                    .map(std::string::ToString::to_string);
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

/// Get raw MCP tool definitions with `input_schema` intact (no lossy
/// conversion).
pub async fn get_raw_mcp_tool_defs(tool: &Tool) -> Result<Vec<McpToolDef>, ToolshedError> {
    let mcp_cfg = tool
        .manifest
        .mcp
        .as_ref()
        .ok_or_else(|| ToolshedError::MissingMcpConfig {
            tool: tool.manifest.name.clone(),
        })?;
    match mcp_cfg.transport {
        McpTransport::Stdio => mcp::stdio::list_tools(tool).await,
        McpTransport::Http => mcp::http::list_tools(tool).await,
    }
}

fn cache_path_for(tool: &Tool) -> PathBuf {
    config::cache_dir().join(format!("{}.tools.json", tool.manifest.name))
}

fn read_cache(path: &Path) -> Option<Vec<McpToolInfo>> {
    let metadata = std::fs::metadata(path).ok()?;
    let modified = metadata.modified().ok()?;
    let age = SystemTime::now().duration_since(modified).ok()?;

    if age.as_secs() > config::INTROSPECT_CACHE_TTL_SECS {
        return None;
    }

    let content = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

fn write_cache(path: &Path, tools: &[McpToolInfo]) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string_pretty(tools) {
        let _ = std::fs::write(path, json);
    }
}
