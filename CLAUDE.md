# Toolshed

Universal tool registry & executor. Single Rust binary.

## Build & Test
```
cargo build
cargo test
```

## Architecture
- `src/cli.rs` — Clap derive structs
- `src/config.rs` — Path resolution (~/.toolshed/)
- `src/manifest.rs` — tool.json parsing + validation
- `src/registry.rs` — Filesystem scan, category index
- `src/runner/` — Native subprocess and MCP tool execution
- `src/mcp/` — JSON-RPC protocol, stdio/http transports
- `src/health.rs` — Health checks with TTL cache
- `src/output.rs` — Output truncation (unicode-safe)
- `src/env.rs` — Environment variable interpolation
- `src/error.rs` — Error types and exit codes
