# Toolshed Agent

You have access to Toolshed, a universal tool registry that provides a consistent interface to native CLI tools and MCP servers.

## Discovery Protocol

Before using any tool, follow these three steps:

1. **List categories** — `toolshed list` shows all available tool categories
2. **List tools in a category** — `toolshed list <category>` shows tools and descriptions
3. **Get tool help** — `toolshed help <tool>` shows commands, parameters, and usage

## Running Tools

```
toolshed run <tool> <command> [args...]
```

**Native tools** use positional args and flags:
```
toolshed run rc search-code "async trait"
toolshed run rc query-docs serde "derive macros"
toolshed run code-search gather /path/to/repo < diff.patch
toolshed run indexer index /path/to/repo --full
```

**MCP tools** use `--name value` pairs:
```
toolshed run github search_repositories --query "language:rust stars:>1000"
toolshed run gitlab list_projects
toolshed run memory create_entities --entities '[{"name":"foo","entityType":"concept","observations":["bar"]}]'
```

### Output Control

- Output is truncated by default (per-tool `max_output` setting)
- Use `--full` to get complete output: `toolshed run <tool> <command> --full [args...]`
- Use `--timeout <seconds>` for long-running commands

## Health Checks

```
toolshed list --health    # show up/down status per category
toolshed validate         # validate all tool manifests
```

## Key Principles

- **Always discover before using.** Don't guess tool names or parameters. Run `toolshed help <tool>` first.
- **Use the right tool for the job.** Check what's available before writing custom scripts.
- **Tools are stateless between calls.** Each `toolshed run` is independent. MCP servers may maintain session state during their idle timeout (5 min default).
- **Environment variables** are interpolated at runtime. If a tool fails with an env var error, the required variable is not set in the shell.

## Tool Categories

The registry is organized by category. Common categories include:

| Category | Examples |
|----------|----------|
| `automation` | Browser automation (agent-browser, playwright) |
| `code-search` | Semantic + keyword code search |
| `database` | Database operations |
| `debugging` | Dev server monitoring, error capture (dev3000) |
| `documentation` | Crate/library documentation search |
| `filesystem` | File read/write/search |
| `indexing` | Codebase and dependency indexing |
| `infrastructure` | Docker, containers |
| `knowledge` | Persistent knowledge graph |
| `project-management` | Issue tracking, task management |
| `reasoning` | LLM reasoning and thinking |
| `utilities` | Time, general utilities |
| `version-control` | Git, GitHub, GitLab operations |
| `web` | URL fetching and content extraction |

Run `toolshed list` for the current set of categories and tools.

## Error Handling

| Exit code | Meaning |
|-----------|---------|
| 0 | Success |
| 1 | Tool/category/command not found |
| 2 | Execution failure (tool crashed, MCP error) |
| 3 | Configuration/validation error |
| 4 | Timeout |
| 5 | Missing environment variable |

## Reference

For full protocol details, see [PROTOCOL.md](references/PROTOCOL.md).
