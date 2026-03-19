pub const MCP_PROTOCOL_VERSION: &str = "2024-11-05";

// ── JSON-RPC ──

#[derive(Debug, Clone)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: u64,
    pub method: String,
    pub params: Option<serde_json::Value>,
}

impl JsonRpcRequest {
    pub fn new(id: u64, method: &str, params: Option<serde_json::Value>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            method: method.to_string(),
            params,
        }
    }
}

impl serde::Serialize for JsonRpcRequest {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let count = 3 + usize::from(self.params.is_some());
        let mut map = serializer.serialize_map(Some(count))?;
        map.serialize_entry("jsonrpc", &self.jsonrpc)?;
        map.serialize_entry("id", &self.id)?;
        map.serialize_entry("method", &self.method)?;
        if let Some(ref p) = self.params { map.serialize_entry("params", p)?; }
        map.end()
    }
}

impl<'de> serde::Deserialize<'de> for JsonRpcRequest {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let v = serde_json::Value::deserialize(deserializer).map_err(serde::de::Error::custom)?;
        let obj = v.as_object().ok_or_else(|| serde::de::Error::custom("expected object"))?;
        Ok(Self {
            jsonrpc: obj.get("jsonrpc").and_then(serde_json::Value::as_str).unwrap_or("2.0").to_string(),
            id: obj.get("id").and_then(serde_json::Value::as_u64).unwrap_or(0),
            method: obj.get("method").and_then(serde_json::Value::as_str).unwrap_or_default().to_string(),
            params: obj.get("params").cloned(),
        })
    }
}

#[derive(Debug, Clone)]
pub struct JsonRpcNotification {
    pub jsonrpc: String,
    pub method: String,
    pub params: Option<serde_json::Value>,
}

impl JsonRpcNotification {
    pub fn new(method: &str, params: Option<serde_json::Value>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            method: method.to_string(),
            params,
        }
    }
}

impl serde::Serialize for JsonRpcNotification {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let count = 2 + usize::from(self.params.is_some());
        let mut map = serializer.serialize_map(Some(count))?;
        map.serialize_entry("jsonrpc", &self.jsonrpc)?;
        map.serialize_entry("method", &self.method)?;
        if let Some(ref p) = self.params { map.serialize_entry("params", p)?; }
        map.end()
    }
}

#[derive(Debug, Clone)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: Option<u64>,
    pub result: Option<serde_json::Value>,
    pub error: Option<JsonRpcError>,
}

impl serde::Serialize for JsonRpcResponse {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let count = 1 + usize::from(self.id.is_some()) + usize::from(self.result.is_some()) + usize::from(self.error.is_some());
        let mut map = serializer.serialize_map(Some(count))?;
        map.serialize_entry("jsonrpc", &self.jsonrpc)?;
        if let Some(id) = self.id { map.serialize_entry("id", &id)?; }
        if let Some(ref r) = self.result { map.serialize_entry("result", r)?; }
        if let Some(ref e) = self.error { map.serialize_entry("error", e)?; }
        map.end()
    }
}

impl<'de> serde::Deserialize<'de> for JsonRpcResponse {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let v = serde_json::Value::deserialize(deserializer).map_err(serde::de::Error::custom)?;
        let obj = v.as_object().ok_or_else(|| serde::de::Error::custom("expected object"))?;
        Ok(Self {
            jsonrpc: obj.get("jsonrpc").and_then(serde_json::Value::as_str).unwrap_or("2.0").to_string(),
            id: obj.get("id").and_then(serde_json::Value::as_u64),
            result: obj.get("result").cloned(),
            error: obj.get("error").map(|v| JsonRpcError::deserialize(v.clone())).transpose().map_err(serde::de::Error::custom)?,
        })
    }
}

#[derive(Debug, Clone)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
    pub data: Option<serde_json::Value>,
}

impl serde::Serialize for JsonRpcError {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let count = 2 + usize::from(self.data.is_some());
        let mut map = serializer.serialize_map(Some(count))?;
        map.serialize_entry("code", &self.code)?;
        map.serialize_entry("message", &self.message)?;
        if let Some(ref d) = self.data { map.serialize_entry("data", d)?; }
        map.end()
    }
}

impl<'de> serde::Deserialize<'de> for JsonRpcError {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let v = serde_json::Value::deserialize(deserializer).map_err(serde::de::Error::custom)?;
        let obj = v.as_object().ok_or_else(|| serde::de::Error::custom("expected object"))?;
        Ok(Self {
            code: obj.get("code").and_then(serde_json::Value::as_i64).unwrap_or(0),
            message: obj.get("message").and_then(serde_json::Value::as_str).unwrap_or_default().to_string(),
            data: obj.get("data").cloned(),
        })
    }
}

// ── MCP Initialize ──

#[derive(Debug, Clone)]
pub struct InitializeParams {
    pub protocol_version: String,
    pub capabilities: ClientCapabilities,
    pub client_info: ClientInfo,
}

#[derive(Debug, Clone)]
pub struct ClientCapabilities {}

#[derive(Debug, Clone)]
pub struct ClientInfo {
    pub name: String,
    pub version: String,
}

impl serde::Serialize for InitializeParams {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(Some(3))?;
        map.serialize_entry("protocolVersion", &self.protocol_version)?;
        map.serialize_entry("capabilities", &self.capabilities)?;
        map.serialize_entry("clientInfo", &self.client_info)?;
        map.end()
    }
}

impl serde::Serialize for ClientCapabilities {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        serializer.serialize_map(Some(0))?.end()
    }
}

impl serde::Serialize for ClientInfo {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(Some(2))?;
        map.serialize_entry("name", &self.name)?;
        map.serialize_entry("version", &self.version)?;
        map.end()
    }
}

impl InitializeParams {
    pub fn default_params() -> Self {
        Self {
            protocol_version: MCP_PROTOCOL_VERSION.to_string(),
            capabilities: ClientCapabilities {},
            client_info: ClientInfo {
                name: "toolshed".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
            },
        }
    }
}

// ── MCP Tools ──

#[derive(Debug, Clone)]
pub struct ToolsCallParams {
    pub name: String,
    pub arguments: serde_json::Value,
}

impl serde::Serialize for ToolsCallParams {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(Some(2))?;
        map.serialize_entry("name", &self.name)?;
        map.serialize_entry("arguments", &self.arguments)?;
        map.end()
    }
}

#[derive(Debug, Clone)]
pub struct ToolsListResult {
    pub tools: Vec<McpToolDef>,
    pub next_cursor: Option<String>,
}

impl<'de> serde::Deserialize<'de> for ToolsListResult {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let v = serde_json::Value::deserialize(deserializer).map_err(serde::de::Error::custom)?;
        let obj = v.as_object().ok_or_else(|| serde::de::Error::custom("expected object"))?;
        let tools = obj.get("tools").map_or_else(|| Ok(Vec::new()), |v| {
            Vec::<McpToolDef>::deserialize(v.clone()).map_err(serde::de::Error::custom)
        })?;
        let next_cursor = obj.get("nextCursor").and_then(serde_json::Value::as_str).map(String::from);
        Ok(Self { tools, next_cursor })
    }
}

#[derive(Debug, Clone)]
pub struct McpToolDef {
    pub name: String,
    pub description: Option<String>,
    pub input_schema: Option<serde_json::Value>,
}

impl serde::Serialize for McpToolDef {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let count = 1 + usize::from(self.description.is_some()) + usize::from(self.input_schema.is_some());
        let mut map = serializer.serialize_map(Some(count))?;
        map.serialize_entry("name", &self.name)?;
        if let Some(ref d) = self.description { map.serialize_entry("description", d)?; }
        if let Some(ref s) = self.input_schema { map.serialize_entry("inputSchema", s)?; }
        map.end()
    }
}

impl<'de> serde::Deserialize<'de> for McpToolDef {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let v = serde_json::Value::deserialize(deserializer).map_err(serde::de::Error::custom)?;
        let obj = v.as_object().ok_or_else(|| serde::de::Error::custom("expected object"))?;
        Ok(Self {
            name: obj.get("name").and_then(serde_json::Value::as_str).unwrap_or_default().to_string(),
            description: obj.get("description").and_then(serde_json::Value::as_str).map(String::from),
            input_schema: obj.get("inputSchema").cloned(),
        })
    }
}

#[derive(Debug, Clone)]
pub struct ToolCallResult {
    pub content: Vec<ContentItem>,
    pub is_error: bool,
}

impl serde::Serialize for ToolCallResult {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(Some(2))?;
        map.serialize_entry("content", &self.content)?;
        map.serialize_entry("isError", &self.is_error)?;
        map.end()
    }
}

impl<'de> serde::Deserialize<'de> for ToolCallResult {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let v = serde_json::Value::deserialize(deserializer).map_err(serde::de::Error::custom)?;
        let obj = v.as_object().ok_or_else(|| serde::de::Error::custom("expected object"))?;
        let content = obj.get("content").map_or_else(|| Ok(Vec::new()), |v| {
            Vec::<ContentItem>::deserialize(v.clone()).map_err(serde::de::Error::custom)
        })?;
        let is_error = obj.get("isError").and_then(serde_json::Value::as_bool).unwrap_or_default();
        Ok(Self { content, is_error })
    }
}

#[derive(Debug, Clone)]
pub enum ContentItem {
    Text { text: String },
    Image { data: String, mime_type: String },
    Resource { resource: serde_json::Value },
}

impl serde::Serialize for ContentItem {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        match self {
            Self::Text { text } => {
                let mut map = serializer.serialize_map(Some(2))?;
                map.serialize_entry("type", "text")?;
                map.serialize_entry("text", text)?;
                map.end()
            }
            Self::Image { data, mime_type } => {
                let mut map = serializer.serialize_map(Some(3))?;
                map.serialize_entry("type", "image")?;
                map.serialize_entry("data", data)?;
                map.serialize_entry("mimeType", mime_type)?;
                map.end()
            }
            Self::Resource { resource } => {
                let mut map = serializer.serialize_map(Some(2))?;
                map.serialize_entry("type", "resource")?;
                map.serialize_entry("resource", resource)?;
                map.end()
            }
        }
    }
}

impl<'de> serde::Deserialize<'de> for ContentItem {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let v = serde_json::Value::deserialize(deserializer).map_err(serde::de::Error::custom)?;
        let obj = v.as_object().ok_or_else(|| serde::de::Error::custom("expected object"))?;
        let tag = obj.get("type").and_then(serde_json::Value::as_str).unwrap_or("");
        match tag {
            "text" => Ok(Self::Text {
                text: obj.get("text").and_then(serde_json::Value::as_str).unwrap_or_default().to_string(),
            }),
            "image" => Ok(Self::Image {
                data: obj.get("data").and_then(serde_json::Value::as_str).unwrap_or_default().to_string(),
                mime_type: obj.get("mimeType").and_then(serde_json::Value::as_str).unwrap_or_default().to_string(),
            }),
            "resource" => Ok(Self::Resource {
                resource: obj.get("resource").cloned().unwrap_or(serde_json::Value::Null),
            }),
            other => Err(serde::de::Error::unknown_variant(other, &["text", "image", "resource"])),
        }
    }
}

impl ContentItem {
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Self::Text { text } => Some(text),
            _ => None,
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_request() {
        let req = JsonRpcRequest::new(1, "initialize", None);
        let json = serde_json::to_string(&req).unwrap();
        let parsed: JsonRpcRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, 1);
        assert_eq!(parsed.method, "initialize");
    }

    #[test]
    fn roundtrip_response_with_result() {
        let json = r#"{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2024-11-05"}}"#;
        let resp: JsonRpcResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.id, Some(1));
        assert!(resp.result.is_some());
        assert!(resp.error.is_none());
    }

    #[test]
    fn roundtrip_response_with_error() {
        let json =
            r#"{"jsonrpc":"2.0","id":2,"error":{"code":-32601,"message":"Method not found"}}"#;
        let resp: JsonRpcResponse = serde_json::from_str(json).unwrap();
        assert!(resp.error.is_some());
        assert_eq!(resp.error.unwrap().code, -32601);
    }

    #[test]
    fn roundtrip_tool_call_result() {
        let json = r#"{"content":[{"type":"text","text":"hello"}],"isError":false}"#;
        let result: ToolCallResult = serde_json::from_str(json).unwrap();
        assert_eq!(result.content.len(), 1);
        assert_eq!(result.content[0].as_text(), Some("hello"));
        assert!(!result.is_error);
    }

    #[test]
    fn parse_tools_list() {
        let json = r#"{"tools":[{"name":"search","description":"Search issues","inputSchema":{"type":"object","properties":{"query":{"type":"string"}},"required":["query"]}}]}"#;
        let result: ToolsListResult = serde_json::from_str(json).unwrap();
        assert_eq!(result.tools.len(), 1);
        assert_eq!(result.tools[0].name, "search");
    }

    #[test]
    fn notification_no_id() {
        let notif = JsonRpcNotification::new("initialized", None);
        let json = serde_json::to_string(&notif).unwrap();
        assert!(!json.contains("\"id\""));
    }
}
