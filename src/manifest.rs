use std::{collections::BTreeMap, fmt, path::Path};

use crate::{config::DEFAULT_MAX_OUTPUT, error::ToolshedError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolType {
    Native,
    Mcp,
}

impl fmt::Display for ToolType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Native => write!(f, "native"),
            Self::Mcp => write!(f, "mcp"),
        }
    }
}

impl<'de> serde::Deserialize<'de> for ToolType {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        match s.as_str() {
            "native" => Ok(Self::Native),
            "mcp" => Ok(Self::Mcp),
            other => Err(serde::de::Error::unknown_variant(other, &["native", "mcp"])),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArgType {
    String,
    Int,
    Float,
    Bool,
}

impl fmt::Display for ArgType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::String => write!(f, "string"),
            Self::Int => write!(f, "int"),
            Self::Float => write!(f, "float"),
            Self::Bool => write!(f, "bool"),
        }
    }
}

impl<'de> serde::Deserialize<'de> for ArgType {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        match s.as_str() {
            "string" => Ok(Self::String),
            "int" => Ok(Self::Int),
            "float" => Ok(Self::Float),
            "bool" => Ok(Self::Bool),
            other => Err(serde::de::Error::unknown_variant(other, &["string", "int", "float", "bool"])),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum McpTransport {
    Stdio,
    Http,
}

impl fmt::Display for McpTransport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Stdio => write!(f, "stdio"),
            Self::Http => write!(f, "http"),
        }
    }
}

impl<'de> serde::Deserialize<'de> for McpTransport {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        match s.as_str() {
            "stdio" => Ok(Self::Stdio),
            "http" => Ok(Self::Http),
            other => Err(serde::de::Error::unknown_variant(other, &["stdio", "http"])),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ArgDef {
    pub arg_type: ArgType,
    pub required: bool,
    pub positional: bool,
    pub default: Option<serde_json::Value>,
    pub description: Option<String>,
}

impl<'de> serde::Deserialize<'de> for ArgDef {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let v = serde_json::Value::deserialize(deserializer).map_err(serde::de::Error::custom)?;
        let obj = v.as_object().ok_or_else(|| serde::de::Error::custom("expected object"))?;
        let type_val = obj.get("type").ok_or_else(|| serde::de::Error::missing_field("type"))?;
        let arg_type = ArgType::deserialize(type_val.clone()).map_err(serde::de::Error::custom)?;
        Ok(Self {
            arg_type,
            required: obj.get("required").and_then(serde_json::Value::as_bool).unwrap_or_default(),
            positional: obj.get("positional").and_then(serde_json::Value::as_bool).unwrap_or_default(),
            default: obj.get("default").cloned(),
            description: obj.get("description").and_then(serde_json::Value::as_str).map(String::from),
        })
    }
}

#[derive(Debug, Clone)]
pub struct CommandDef {
    pub description: String,
    pub args: BTreeMap<String, ArgDef>,
}

impl<'de> serde::Deserialize<'de> for CommandDef {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let v = serde_json::Value::deserialize(deserializer).map_err(serde::de::Error::custom)?;
        let obj = v.as_object().ok_or_else(|| serde::de::Error::custom("expected object"))?;
        let description = obj.get("description").and_then(serde_json::Value::as_str)
            .ok_or_else(|| serde::de::Error::missing_field("description"))?.to_string();
        let args = obj.get("args").map_or_else(|| Ok(BTreeMap::new()), |v| {
            BTreeMap::<String, ArgDef>::deserialize(v.clone()).map_err(serde::de::Error::custom)
        })?;
        Ok(Self { description, args })
    }
}

#[derive(Debug, Clone)]
pub struct McpConfig {
    pub transport: McpTransport,
    pub command: Option<String>,
    pub args: Vec<String>,
    pub url: Option<String>,
    pub env: BTreeMap<String, String>,
    pub headers: BTreeMap<String, String>,
}

impl<'de> serde::Deserialize<'de> for McpConfig {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let v = serde_json::Value::deserialize(deserializer).map_err(serde::de::Error::custom)?;
        let obj = v.as_object().ok_or_else(|| serde::de::Error::custom("expected object"))?;
        let tv = obj.get("transport").ok_or_else(|| serde::de::Error::missing_field("transport"))?;
        let transport = McpTransport::deserialize(tv.clone()).map_err(serde::de::Error::custom)?;
        let command = obj.get("command").and_then(serde_json::Value::as_str).map(String::from);
        let args = obj.get("args").map_or_else(|| Ok(Vec::new()), |v| {
            Vec::<String>::deserialize(v.clone()).map_err(serde::de::Error::custom)
        })?;
        let url = obj.get("url").and_then(serde_json::Value::as_str).map(String::from);
        let env = obj.get("env").map_or_else(|| Ok(BTreeMap::new()), |v| {
            BTreeMap::<String, String>::deserialize(v.clone()).map_err(serde::de::Error::custom)
        })?;
        let headers = obj.get("headers").map_or_else(|| Ok(BTreeMap::new()), |v| {
            BTreeMap::<String, String>::deserialize(v.clone()).map_err(serde::de::Error::custom)
        })?;
        Ok(Self { transport, command, args, url, env, headers })
    }
}

#[derive(Debug, Clone)]
pub struct ToolManifest {
    pub name: String,
    pub description: String,
    pub category: String,
    pub tool_type: ToolType,
    pub max_output: usize,
    pub health: Option<String>,
    pub commands: BTreeMap<String, CommandDef>,
    pub mcp: Option<McpConfig>,
}

impl<'de> serde::Deserialize<'de> for ToolManifest {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let v = serde_json::Value::deserialize(deserializer).map_err(serde::de::Error::custom)?;
        let obj = v.as_object().ok_or_else(|| serde::de::Error::custom("expected object"))?;
        let name = obj.get("name").and_then(serde_json::Value::as_str)
            .ok_or_else(|| serde::de::Error::missing_field("name"))?.to_string();
        let description = obj.get("description").and_then(serde_json::Value::as_str)
            .ok_or_else(|| serde::de::Error::missing_field("description"))?.to_string();
        let category = obj.get("category").and_then(serde_json::Value::as_str)
            .ok_or_else(|| serde::de::Error::missing_field("category"))?.to_string();
        let tv = obj.get("type").ok_or_else(|| serde::de::Error::missing_field("type"))?;
        let tool_type = ToolType::deserialize(tv.clone()).map_err(serde::de::Error::custom)?;
        let max_output = obj.get("max_output").and_then(serde_json::Value::as_u64)
            .and_then(|v| usize::try_from(v).ok())
            .unwrap_or(DEFAULT_MAX_OUTPUT);
        let health = obj.get("health").and_then(serde_json::Value::as_str).map(String::from);
        let commands = obj.get("commands").map_or_else(|| Ok(BTreeMap::new()), |v| {
            BTreeMap::<String, CommandDef>::deserialize(v.clone()).map_err(serde::de::Error::custom)
        })?;
        let mcp = obj.get("mcp").map(|v| McpConfig::deserialize(v.clone()))
            .transpose().map_err(serde::de::Error::custom)?;
        Ok(Self { name, description, category, tool_type, max_output, health, commands, mcp })
    }
}

impl ToolManifest {
    pub fn load_and_validate(path: &Path, dir_name: &str) -> Result<Self, ToolshedError> {
        let content = std::fs::read_to_string(path).map_err(|e| ToolshedError::BadManifest {
            tool: dir_name.to_string(),
            reason: format!("cannot read tool.json: {e}"),
        })?;

        let manifest: Self =
            serde_json::from_str(&content).map_err(|e| ToolshedError::BadManifest {
                tool: dir_name.to_string(),
                reason: format!("invalid JSON: {e}"),
            })?;

        manifest.validate(dir_name)?;
        Ok(manifest)
    }

    fn validate(&self, dir_name: &str) -> Result<(), ToolshedError> {
        let err = |reason: String| ToolshedError::BadManifest {
            tool: dir_name.to_string(),
            reason,
        };

        // Name must match directory
        if self.name != dir_name {
            return Err(err(format!(
                "name '{}' does not match directory '{dir_name}'",
                self.name
            )));
        }

        // Name format
        if !is_valid_name(&self.name) {
            return Err(err(format!(
                "name must be 1-64 chars, [a-z0-9_-] only, got '{}'",
                self.name
            )));
        }

        // Description
        if self.description.is_empty() {
            return Err(err("description cannot be empty".to_string()));
        }
        if self.description.len() > 200 {
            return Err(err("description exceeds 200 chars".to_string()));
        }

        // Category format
        if !is_valid_category(&self.category) {
            return Err(err(format!(
                "category must be 1-32 chars, [a-z0-9-] only, got '{}'",
                self.category
            )));
        }

        // max_output
        if self.max_output < 256 {
            return Err(err(format!(
                "max_output must be >= 256, got {}",
                self.max_output
            )));
        }

        // Type-specific validation
        match self.tool_type {
            ToolType::Native => {
                if self.commands.is_empty() {
                    return Err(err(
                        "native tools must have at least one command".to_string()
                    ));
                }
                if self.mcp.is_some() {
                    return Err(err("native tools must not have 'mcp' config".to_string()));
                }
            }
            ToolType::Mcp => {
                let mcp = self
                    .mcp
                    .as_ref()
                    .ok_or_else(|| err("MCP tools require 'mcp' config".to_string()))?;

                match mcp.transport {
                    McpTransport::Stdio => {
                        if mcp.command.is_none() {
                            return Err(err("MCP stdio transport requires 'command'".to_string()));
                        }
                    }
                    McpTransport::Http => {
                        if mcp.url.is_none() {
                            return Err(err("MCP http transport requires 'url'".to_string()));
                        }
                    }
                }
            }
        }

        Ok(())
    }
}

pub fn is_valid_name(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
}

fn is_valid_category(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 32
        && s.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn valid_native_json() -> &'static str {
        r#"{
            "name": "test-tool",
            "description": "A test tool",
            "category": "testing",
            "type": "native",
            "commands": {
                "run": {
                    "description": "Run something",
                    "args": {
                        "query": {
                            "type": "string",
                            "required": true,
                            "positional": true,
                            "description": "The query"
                        }
                    }
                }
            }
        }"#
    }

    fn valid_mcp_stdio_json() -> &'static str {
        r#"{
            "name": "mcp-test",
            "description": "An MCP test tool",
            "category": "testing",
            "type": "mcp",
            "mcp": {
                "transport": "stdio",
                "command": "node",
                "args": ["server.js"],
                "env": {"API_KEY": "${MY_KEY}"}
            }
        }"#
    }

    fn valid_mcp_http_json() -> &'static str {
        r#"{
            "name": "mcp-http",
            "description": "An MCP HTTP tool",
            "category": "testing",
            "type": "mcp",
            "mcp": {
                "transport": "http",
                "url": "https://example.com/mcp"
            }
        }"#
    }

    #[test]
    fn parse_valid_native() {
        let m: ToolManifest = serde_json::from_str(valid_native_json()).unwrap();
        m.validate("test-tool").unwrap();
        assert_eq!(m.tool_type, ToolType::Native);
        assert_eq!(m.max_output, DEFAULT_MAX_OUTPUT);
        assert_eq!(m.commands.len(), 1);
        let cmd = &m.commands["run"];
        assert!(cmd.args["query"].required);
        assert!(cmd.args["query"].positional);
    }

    #[test]
    fn parse_valid_mcp_stdio() {
        let m: ToolManifest = serde_json::from_str(valid_mcp_stdio_json()).unwrap();
        m.validate("mcp-test").unwrap();
        assert_eq!(m.tool_type, ToolType::Mcp);
        let mcp = m.mcp.unwrap();
        assert_eq!(mcp.command.unwrap(), "node");
        assert_eq!(mcp.args, vec!["server.js"]);
    }

    #[test]
    fn parse_valid_mcp_http() {
        let m: ToolManifest = serde_json::from_str(valid_mcp_http_json()).unwrap();
        m.validate("mcp-http").unwrap();
        let mcp = m.mcp.unwrap();
        assert_eq!(mcp.url.unwrap(), "https://example.com/mcp");
    }

    #[test]
    fn name_mismatch() {
        let m: ToolManifest = serde_json::from_str(valid_native_json()).unwrap();
        let err = m.validate("wrong-name").unwrap_err();
        assert!(err.to_string().contains("does not match directory"));
    }

    #[test]
    fn invalid_name_chars() {
        let json = r#"{
            "name": "Bad Tool!",
            "description": "bad",
            "category": "testing",
            "type": "native",
            "commands": {"x": {"description": "x"}}
        }"#;
        let m: ToolManifest = serde_json::from_str(json).unwrap();
        let err = m.validate("Bad Tool!").unwrap_err();
        assert!(err.to_string().contains("[a-z0-9_-]"));
    }

    #[test]
    fn empty_description() {
        let json = r#"{
            "name": "x",
            "description": "",
            "category": "testing",
            "type": "native",
            "commands": {"x": {"description": "x"}}
        }"#;
        let m: ToolManifest = serde_json::from_str(json).unwrap();
        let err = m.validate("x").unwrap_err();
        assert!(err.to_string().contains("empty"));
    }

    #[test]
    fn native_no_commands() {
        let json = r#"{
            "name": "x",
            "description": "desc",
            "category": "testing",
            "type": "native",
            "commands": {}
        }"#;
        let m: ToolManifest = serde_json::from_str(json).unwrap();
        let err = m.validate("x").unwrap_err();
        assert!(err.to_string().contains("at least one command"));
    }

    #[test]
    fn mcp_missing_config() {
        let json = r#"{
            "name": "x",
            "description": "desc",
            "category": "testing",
            "type": "mcp"
        }"#;
        let m: ToolManifest = serde_json::from_str(json).unwrap();
        let err = m.validate("x").unwrap_err();
        assert!(err.to_string().contains("require 'mcp' config"));
    }

    #[test]
    fn mcp_stdio_missing_command() {
        let json = r#"{
            "name": "x",
            "description": "desc",
            "category": "testing",
            "type": "mcp",
            "mcp": { "transport": "stdio" }
        }"#;
        let m: ToolManifest = serde_json::from_str(json).unwrap();
        let err = m.validate("x").unwrap_err();
        assert!(err.to_string().contains("requires 'command'"));
    }

    #[test]
    fn mcp_http_missing_url() {
        let json = r#"{
            "name": "x",
            "description": "desc",
            "category": "testing",
            "type": "mcp",
            "mcp": { "transport": "http" }
        }"#;
        let m: ToolManifest = serde_json::from_str(json).unwrap();
        let err = m.validate("x").unwrap_err();
        assert!(err.to_string().contains("requires 'url'"));
    }

    #[test]
    fn max_output_too_small() {
        let json = r#"{
            "name": "x",
            "description": "desc",
            "category": "testing",
            "type": "native",
            "max_output": 100,
            "commands": {"x": {"description": "x"}}
        }"#;
        let m: ToolManifest = serde_json::from_str(json).unwrap();
        let err = m.validate("x").unwrap_err();
        assert!(err.to_string().contains(">= 256"));
    }
}
