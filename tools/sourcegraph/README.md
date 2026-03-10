# sourcegraph

Code search, file browsing, and commit exploration tool powered by a Sourcegraph instance via its GraphQL and search APIs.

## Prerequisites

- **Sourcegraph instance** with API access
- **HashiCorp Vault** storing credentials at `secret/data/sourcegraph` with keys `url` and `token`
- `curl`, `jq` available on PATH

## Configuration

Credentials are loaded from Vault automatically. Override the Vault address and token via environment variables:

```bash
export VAULT_ADDR="http://127.0.0.1:8200"    # default
export VAULT_TOKEN="toolshed-dev-token"        # default
```

The Vault secret at `secret/data/sourcegraph` must contain:

```json
{
  "url": "https://sourcegraph.example.com",
  "token": "sgp_xxxxxxxxxxxx"
}
```

## Commands

### keyword_search

Search code across all indexed repositories.

```bash
sourcegraph keyword_search "func main" --count 10 --format json
sourcegraph keyword_search "repo:myorg/ lang:go error handling"
```

### read_file

Read the contents of a file at a specific revision.

```bash
sourcegraph read_file github.com/org/repo --path src/main.go
sourcegraph read_file github.com/org/repo --path README.md --rev v1.2.0
```

### list_repos

List repositories indexed by the Sourcegraph instance.

```bash
sourcegraph list_repos
sourcegraph list_repos --query "myorg" --count 50
```

### list_files

List files and directories at a path within a repository.

```bash
sourcegraph list_files github.com/org/repo
sourcegraph list_files github.com/org/repo --path src/pkg --rev main
```

### symbols

Search for code symbols (functions, classes, variables).

```bash
sourcegraph symbols "handleRequest" --repo github.com/org/repo
sourcegraph symbols "Config" --kind class --count 10
```

### commit_search

Search commits by message, author, content, and date ranges.

```bash
sourcegraph commit_search "fix bug" --repo github.com/org/repo
sourcegraph commit_search "migration" --author "alice" --after 2024-01-01 --before 2024-06-30
sourcegraph commit_search "breaking change" --format json
```

### diff_search

Search actual code changes (diffs) for patterns.

```bash
sourcegraph diff_search "deprecated_function" --repo github.com/org/repo
sourcegraph diff_search "API_KEY" --after 2024-01-01 --count 50
```

### compare_revisions

Compare changes between two revisions in a repository.

```bash
sourcegraph compare_revisions github.com/org/repo --base v1.0.0 --head v2.0.0
sourcegraph compare_revisions github.com/org/repo --base abc123 --head def456 --format json
```

### get_contributor_repos

Find repositories where a specific contributor has made commits.

```bash
sourcegraph get_contributor_repos "alice@example.com"
sourcegraph get_contributor_repos "Alice Smith" --count 100
```

### health

Check connectivity to Sourcegraph and Vault.

```bash
sourcegraph health
```
