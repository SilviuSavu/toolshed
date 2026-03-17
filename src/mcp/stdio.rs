use crate::env;
use crate::error::ToolshedError;
use crate::mcp::protocol::*;
use crate::registry::Tool;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};

struct McpStdioSession {
    child: Child,
    stdin: tokio::process::ChildStdin,
    reader: BufReader<tokio::process::ChildStdout>,
    next_id: u64,
    tool_name: String,
}

impl McpStdioSession {
    async fn spawn(tool: &Tool) -> Result<Self, ToolshedError> {
        let mcp_cfg = tool.manifest.mcp.as_ref().unwrap();
        let command = mcp_cfg.command.as_ref().unwrap();

        let env_vars = env::interpolate_map(&mcp_cfg.env)?;

        let mut cmd = Command::new(command);
        cmd.args(&mcp_cfg.args);
        cmd.envs(env_vars);
        cmd.stdin(std::process::Stdio::piped());
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        let mut child = cmd.spawn().map_err(|e| ToolshedError::McpSpawnFailed {
            tool: tool.manifest.name.clone(),
            reason: e.to_string(),
        })?;

        let stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let reader = BufReader::new(stdout);

        Ok(Self {
            child,
            stdin,
            reader,
            next_id: 1,
            tool_name: tool.manifest.name.clone(),
        })
    }

    async fn initialize(&mut self) -> Result<(), ToolshedError> {
        let params = InitializeParams::default_params();
        let _response = self
            .send_request("initialize", Some(serde_json::to_value(params).unwrap()))
            .await?;

        // Send initialized notification
        let notif = JsonRpcNotification::new("notifications/initialized", None);
        self.send_raw(&serde_json::to_string(&notif).unwrap())
            .await?;

        Ok(())
    }

    async fn send_request(
        &mut self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value, ToolshedError> {
        let id = self.next_id;
        self.next_id += 1;

        let req = JsonRpcRequest::new(id, method, params);
        let json = serde_json::to_string(&req).unwrap();
        self.send_raw(&json).await?;

        // Read responses, skipping notifications (no id)
        loop {
            let line = self.read_line().await?;
            // Try to parse as response
            if let Ok(resp) = serde_json::from_str::<JsonRpcResponse>(&line) {
                if resp.id == Some(id) {
                    if let Some(err) = resp.error {
                        return Err(ToolshedError::McpRpcError {
                            tool: self.tool_name.clone(),
                            code: err.code,
                            message: err.message,
                        });
                    }
                    return resp.result.ok_or_else(|| ToolshedError::McpBadResponse {
                        tool: self.tool_name.clone(),
                        reason: "response has no result".to_string(),
                    });
                }
                // Response for different id — skip
            }
            // Not a response or different id — skip (likely notification)
        }
    }

    async fn send_raw(&mut self, json: &str) -> Result<(), ToolshedError> {
        self.stdin
            .write_all(json.as_bytes())
            .await
            .map_err(|e| ToolshedError::McpSpawnFailed {
                tool: self.tool_name.clone(),
                reason: e.to_string(),
            })?;
        self.stdin
            .write_all(b"\n")
            .await
            .map_err(|e| ToolshedError::McpSpawnFailed {
                tool: self.tool_name.clone(),
                reason: e.to_string(),
            })?;
        self.stdin
            .flush()
            .await
            .map_err(|e| ToolshedError::McpSpawnFailed {
                tool: self.tool_name.clone(),
                reason: e.to_string(),
            })?;
        Ok(())
    }

    async fn read_line(&mut self) -> Result<String, ToolshedError> {
        let mut line = String::new();
        let n =
            self.reader
                .read_line(&mut line)
                .await
                .map_err(|e| ToolshedError::McpBadResponse {
                    tool: self.tool_name.clone(),
                    reason: e.to_string(),
                })?;
        if n == 0 {
            return Err(ToolshedError::McpCrashed {
                tool: self.tool_name.clone(),
            });
        }
        Ok(line)
    }

    async fn shutdown(mut self) {
        let _ = self.child.kill().await;
        let _ = self.child.wait().await;
    }
}

/// List tools from an MCP stdio server.
pub async fn list_tools(tool: &Tool) -> Result<Vec<McpToolDef>, ToolshedError> {
    let mut session = McpStdioSession::spawn(tool).await?;
    session.initialize().await?;

    let mut all_tools = Vec::new();
    let mut cursor: Option<String> = None;

    loop {
        let params = cursor.as_ref().map(|c| serde_json::json!({ "cursor": c }));
        let result = session.send_request("tools/list", params).await?;
        let list: ToolsListResult =
            serde_json::from_value(result).map_err(|e| ToolshedError::McpBadResponse {
                tool: tool.manifest.name.clone(),
                reason: format!("bad tools/list response: {e}"),
            })?;
        all_tools.extend(list.tools);
        match list.next_cursor {
            Some(c) if !c.is_empty() => cursor = Some(c),
            _ => break,
        }
    }

    session.shutdown().await;
    Ok(all_tools)
}

/// Call a single tool on an MCP stdio server.
pub async fn call_tool(
    tool: &Tool,
    tool_name: &str,
    arguments: serde_json::Value,
    _timeout: Option<u64>,
) -> Result<String, ToolshedError> {
    let mut session = McpStdioSession::spawn(tool).await?;
    session.initialize().await?;

    let params = ToolsCallParams {
        name: tool_name.to_string(),
        arguments,
    };

    let result = session
        .send_request("tools/call", Some(serde_json::to_value(params).unwrap()))
        .await?;

    let call_result: ToolCallResult =
        serde_json::from_value(result).map_err(|e| ToolshedError::McpBadResponse {
            tool: tool.manifest.name.clone(),
            reason: format!("bad tools/call response: {e}"),
        })?;

    session.shutdown().await;

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

    let text = call_result
        .content
        .iter()
        .filter_map(|c| c.as_text())
        .collect::<Vec<_>>()
        .join("\n");

    Ok(text)
}
