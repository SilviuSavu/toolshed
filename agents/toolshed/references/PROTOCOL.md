# Toolshed Protocol Reference

## Architecture

Toolshed is a universal tool registry and executor. It provides a single CLI (`toolshed`) that wraps:

- **Native tools**: Shell scripts with structured command definitions
- **MCP tools**: Model Context Protocol servers (stdio or HTTP transport)

All tools are registered in `~/.toolshed/tools/<name>/tool.json`.

## Tool Manifest Format

### Native Tool

```json
{
  "name": "tool-name",
  "description": "What the tool does (max 200 chars)",
  "category": "category-name",
  "type": "native",
  "max_output": 8192,
  "health": "command-to-check-health",
  "commands": {
    "command-name": {
      "description": "What this command does",
      "args": {
        "arg-name": {
          "type": "string|int|float|bool",
          "required": true,
          "positional": true,
          "description": "What this arg is"
        },
        "--flag-name": {
          "type": "bool",
          "required": false,
          "positional": false,
          "default": false,
          "description": "What this flag does"
        }
      }
    }
  }
}
```

Native tools require an executable `run` script in the tool directory.

### MCP Tool

```json
{
  "name": "tool-name",
  "description": "What the tool does",
  "category": "category-name",
  "type": "mcp",
  "max_output": 8192,
  "mcp": {
    "transport": "stdio",
    "command": "npx",
    "args": ["-y", "@scope/package-name"],
    "env": {
      "API_TOKEN": "${ENV_VAR_NAME}"
    }
  }
}
```

MCP tools are auto-discovered via the MCP `tools/list` RPC. No `run` script needed.

## CLI Commands

### `toolshed list [category] [--health]`

Without arguments: list all categories with tool counts.
With category: list tools in that category with descriptions.
With `--health`: show up/down status per category.

### `toolshed help <tool> [command]`

Show detailed help for a tool. For native tools, shows all commands and their arguments. For MCP tools, discovers and shows all available MCP tools with parameters.

### `toolshed run <tool> <command> [--full] [--timeout N] [args...]`

Execute a tool command.

- `--full`: disable output truncation
- `--timeout N`: override default 120s timeout

**Native tool argument passing:**
- Positional args are passed in order
- Flags are passed as `--flag-name value`
- Bool flags: `--flag` (true) or omitted (false/default)

**MCP tool argument passing:**
- All args as `--name value`
- JSON values are parsed automatically
- Arrays/objects must be valid JSON strings

### `toolshed validate [tool]`

Validate tool manifests. Checks JSON syntax, required fields, naming rules, and structural constraints.

### `toolshed agent-prompt [--format plain|skill]`

Generate a system prompt listing all registered tools. Used for agent bootstrapping.

## Environment Variable Interpolation

Tool configs support `${VAR}` and `${VAR:-default}` syntax in MCP env vars and HTTP headers. Variables are resolved at runtime from the shell environment.

## Output Truncation

Output is truncated to `max_output` characters (default 4096) unless `--full` is passed. Truncation is:
- Character-based (not byte-based)
- UTF-8 safe (never splits multi-byte characters)
- Prefers newline boundaries
- Appends a notice with total character count

## Health Checks

Tools can define a `health` field with a shell command. Health results are cached for 30 seconds. The command must exit 0 for "up", non-zero for "down".

## MCP Protocol

Toolshed implements MCP protocol version `2024-11-05`:

1. Initialize connection (`initialize` RPC)
2. Send `notifications/initialized`
3. Discover tools (`tools/list` RPC, paginated)
4. Call tools (`tools/call` RPC with `{ name, arguments }`)

Tool discovery results are cached for 1 hour per tool in `~/.toolshed/cache/`.

## Registry Structure

```
~/.toolshed/
  tools/
    <tool-name>/
      tool.json     # manifest (required)
      run           # executable (native tools only)
  cache/
    <tool-name>.tools.json  # MCP tool cache
```

The registry scans all directories under `~/.toolshed/tools/` at startup. Invalid manifests are collected as errors but don't prevent other tools from loading.
