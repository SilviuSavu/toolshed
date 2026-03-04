use crate::config::DEFAULT_TOOL_TIMEOUT_SECS;
use crate::env;
use crate::error::ToolshedError;
use crate::mcp::protocol::*;
use crate::registry::Tool;
use std::time::Duration;

/// Call a tool via MCP streamable HTTP transport.
pub async fn call_tool(
    tool: &Tool,
    tool_name: &str,
    arguments: serde_json::Value,
    timeout: Option<u64>,
) -> Result<String, ToolshedError> {
    let mcp_cfg = tool.manifest.mcp.as_ref().unwrap();
    let url = mcp_cfg.url.as_ref().unwrap();
    let headers = env::interpolate_map(&mcp_cfg.headers)?;
    let timeout_secs = timeout.unwrap_or(DEFAULT_TOOL_TIMEOUT_SECS);

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(timeout_secs))
        .build()
        .map_err(|e| ToolshedError::McpHttpError {
            tool: tool.manifest.name.clone(),
            reason: e.to_string(),
        })?;

    // Step 1: Initialize
    let init_params = InitializeParams::default_params();
    let init_req = JsonRpcRequest::new(1, "initialize", Some(serde_json::to_value(init_params).unwrap()));
    let init_resp = send_rpc(&client, url, &headers, &init_req, &tool.manifest.name).await?;

    // Extract session ID if present
    let session_id = init_resp
        .headers()
        .get("mcp-session-id")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let _init_result: JsonRpcResponse = parse_json_response(init_resp, &tool.manifest.name).await?;

    // Step 2: Send initialized notification
    let init_notif = JsonRpcNotification::new("notifications/initialized", None);
    let mut notif_builder = client
        .post(url)
        .json(&init_notif);
    for (k, v) in &headers {
        notif_builder = notif_builder.header(k, v);
    }
    if let Some(sid) = &session_id {
        notif_builder = notif_builder.header("mcp-session-id", sid);
    }
    let _ = notif_builder.send().await; // notifications may not return a body

    // Step 3: Call tool
    let call_params = ToolsCallParams {
        name: tool_name.to_string(),
        arguments,
    };
    let call_req = JsonRpcRequest::new(
        2,
        "tools/call",
        Some(serde_json::to_value(call_params).unwrap()),
    );

    let mut req_builder = client.post(url).json(&call_req);
    for (k, v) in &headers {
        req_builder = req_builder.header(k, v);
    }
    if let Some(sid) = &session_id {
        req_builder = req_builder.header("mcp-session-id", sid);
    }

    let resp = req_builder.send().await.map_err(|e| ToolshedError::McpHttpError {
        tool: tool.manifest.name.clone(),
        reason: e.to_string(),
    })?;

    let rpc_resp: JsonRpcResponse = parse_json_response(resp, &tool.manifest.name).await?;

    if let Some(err) = rpc_resp.error {
        return Err(ToolshedError::McpRpcError {
            tool: tool.manifest.name.clone(),
            code: err.code,
            message: err.message,
        });
    }

    let result_val = rpc_resp.result.ok_or_else(|| ToolshedError::McpBadResponse {
        tool: tool.manifest.name.clone(),
        reason: "no result in tools/call response".to_string(),
    })?;

    let call_result: ToolCallResult =
        serde_json::from_value(result_val).map_err(|e| ToolshedError::McpBadResponse {
            tool: tool.manifest.name.clone(),
            reason: format!("bad tools/call response: {e}"),
        })?;

    if call_result.is_error {
        let text = call_result
            .content
            .iter()
            .filter_map(|c| c.as_text())
            .collect::<Vec<_>>()
            .join("\n");
        return Err(ToolshedError::McpRpcError {
            tool: tool.manifest.name.clone(),
            code: -1,
            message: text,
        });
    }

    Ok(call_result
        .content
        .iter()
        .filter_map(|c| c.as_text())
        .collect::<Vec<_>>()
        .join("\n"))
}

/// List tools via MCP HTTP.
pub async fn list_tools(tool: &Tool) -> Result<Vec<McpToolDef>, ToolshedError> {
    let mcp_cfg = tool.manifest.mcp.as_ref().unwrap();
    let url = mcp_cfg.url.as_ref().unwrap();
    let headers = env::interpolate_map(&mcp_cfg.headers)?;
    let timeout_secs = 30u64;

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(timeout_secs))
        .build()
        .map_err(|e| ToolshedError::McpHttpError {
            tool: tool.manifest.name.clone(),
            reason: e.to_string(),
        })?;

    // Initialize
    let init_params = InitializeParams::default_params();
    let init_req = JsonRpcRequest::new(1, "initialize", Some(serde_json::to_value(init_params).unwrap()));
    let init_resp = send_rpc(&client, url, &headers, &init_req, &tool.manifest.name).await?;

    let session_id = init_resp
        .headers()
        .get("mcp-session-id")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let _: JsonRpcResponse = parse_json_response(init_resp, &tool.manifest.name).await?;

    // Initialized notification
    let init_notif = JsonRpcNotification::new("notifications/initialized", None);
    let mut notif_builder = client.post(url).json(&init_notif);
    for (k, v) in &headers {
        notif_builder = notif_builder.header(k, v);
    }
    if let Some(sid) = &session_id {
        notif_builder = notif_builder.header("mcp-session-id", sid);
    }
    let _ = notif_builder.send().await;

    // List tools (paginated)
    let mut all_tools = Vec::new();
    let mut cursor: Option<String> = None;
    let mut req_id = 2u64;

    loop {
        let params = cursor
            .as_ref()
            .map(|c| serde_json::json!({ "cursor": c }));
        let list_req = JsonRpcRequest::new(req_id, "tools/list", params);
        req_id += 1;

        let mut req_builder = client.post(url).json(&list_req);
        for (k, v) in &headers {
            req_builder = req_builder.header(k, v);
        }
        if let Some(sid) = &session_id {
            req_builder = req_builder.header("mcp-session-id", sid);
        }

        let resp = req_builder.send().await.map_err(|e| ToolshedError::McpHttpError {
            tool: tool.manifest.name.clone(),
            reason: e.to_string(),
        })?;

        let rpc_resp: JsonRpcResponse = parse_json_response(resp, &tool.manifest.name).await?;

        if let Some(err) = rpc_resp.error {
            return Err(ToolshedError::McpRpcError {
                tool: tool.manifest.name.clone(),
                code: err.code,
                message: err.message,
            });
        }

        let result_val = rpc_resp.result.ok_or_else(|| ToolshedError::McpBadResponse {
            tool: tool.manifest.name.clone(),
            reason: "no result in tools/list response".to_string(),
        })?;

        let list: ToolsListResult =
            serde_json::from_value(result_val).map_err(|e| ToolshedError::McpBadResponse {
                tool: tool.manifest.name.clone(),
                reason: format!("bad tools/list response: {e}"),
            })?;

        all_tools.extend(list.tools);
        match list.next_cursor {
            Some(c) if !c.is_empty() => cursor = Some(c),
            _ => break,
        }
    }

    Ok(all_tools)
}

async fn send_rpc(
    client: &reqwest::Client,
    url: &str,
    headers: &std::collections::BTreeMap<String, String>,
    req: &JsonRpcRequest,
    tool_name: &str,
) -> Result<reqwest::Response, ToolshedError> {
    let mut builder = client.post(url).json(req);
    for (k, v) in headers {
        builder = builder.header(k, v);
    }

    builder.send().await.map_err(|e| ToolshedError::McpHttpError {
        tool: tool_name.to_string(),
        reason: e.to_string(),
    })
}

async fn parse_json_response(
    resp: reqwest::Response,
    tool_name: &str,
) -> Result<JsonRpcResponse, ToolshedError> {
    let text = resp.text().await.map_err(|e| ToolshedError::McpHttpError {
        tool: tool_name.to_string(),
        reason: e.to_string(),
    })?;

    serde_json::from_str(&text).map_err(|e| ToolshedError::McpBadResponse {
        tool: tool_name.to_string(),
        reason: format!("invalid JSON-RPC response: {e}"),
    })
}
