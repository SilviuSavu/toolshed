pub mod mcp;
pub mod native;

use crate::error::ToolshedError;
use crate::manifest::ToolType;
use crate::registry::Tool;

pub async fn run(
    tool: &Tool,
    command: &str,
    args: &[String],
    timeout: Option<u64>,
) -> Result<String, ToolshedError> {
    match tool.manifest.tool_type {
        ToolType::Native => native::run(tool, command, args, timeout).await,
        ToolType::Mcp => mcp::run(tool, command, args, timeout).await,
    }
}
