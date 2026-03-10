# Sourcegraph Tool

Code search and navigation across all indexed repositories via Sourcegraph GraphQL API.

## Commands

| Command   | Description |
|-----------|-------------|
| `search`  | Search code across all indexed repos. Supports literal, regex (`patternType:regexp`), and structural (`patternType:structural`) queries. Filter by `repo:`, `file:`, `lang:`. |
| `read`    | Read a file from a repository at a specific path. |
| `repos`   | List all indexed repositories. Optional `--filter` for name substring matching. |
| `symbols` | Search for symbol definitions (functions, classes, types). Optional `--repo` to scope. |
| `health`  | Check Sourcegraph connectivity, version, and index status. |

## Credentials

Loaded at runtime from Vault (`secret/data/sourcegraph`):
- `url` — Sourcegraph instance URL
- `token` — API access token

## Examples

```bash
# Search for async functions in Rust
sourcegraph search "lang:rust async fn"

# Read a specific file
sourcegraph read "100.124.182.121/root/api-router" "src/main.rs"

# List repos matching a filter
sourcegraph repos --filter "api"

# Find symbol definitions
sourcegraph symbols "Config" --repo "100.124.182.121/root/toolshed"
```
