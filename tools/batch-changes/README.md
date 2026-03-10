# batch-changes

A toolshed native tool that automates multi-repo changes. It uses Sourcegraph to discover repositories matching a search query, then clones each repo, applies a shell script, commits the changes, pushes a branch, and opens GitLab merge requests.

## Prerequisites

- **jq** — used for all JSON operations
- **git** — with SSH key at `~/.ssh/id_ed25519`
- **curl** — for API calls
- **Vault** — running at `http://127.0.0.1:8200` (configurable via `VAULT_ADDR`)
- **Sourcegraph** — credentials stored in Vault at `secret/data/sourcegraph`
- **GitLab** — token stored in Vault at `secret/data/gitlab`, instance at `http://192.168.1.10`

## Vault Secrets

| Path | Fields | Description |
|---|---|---|
| `secret/data/sourcegraph` | `url`, `token` | Sourcegraph API endpoint and access token |
| `secret/data/gitlab` | `value` | GitLab personal access token |

Default Vault credentials: `VAULT_ADDR=http://127.0.0.1:8200`, `VAULT_TOKEN=toolshed-dev-token`.

## Commands

### create

Create a batch change spec (saved locally, not yet executed).

```bash
batch-changes create \
  --name upgrade-lodash \
  --description "Upgrade lodash to 4.17.21 across all services" \
  --query "repo:myorg/ file:package.json lodash" \
  --script "sed -i 's/\"lodash\": \".*\"/\"lodash\": \"^4.17.21\"/' package.json" \
  --branch "batch/upgrade-lodash" \
  --commit-msg "chore: upgrade lodash to 4.17.21"
```

Specs are saved to `~/.toolshed/data/batch-changes/{name}.json`.

### run

Execute a batch change: clone repos, apply the script, commit, push, and open MRs.

```bash
batch-changes run upgrade-lodash
batch-changes run upgrade-lodash --dry-run
```

Repos are cloned to `/tmp/batch-changes/{name}/` and cleaned up after execution (unless `--dry-run` is used). Results are saved to `~/.toolshed/data/batch-changes/{name}-results.json`.

### status

Check the current state of all MRs for a batch change.

```bash
batch-changes status upgrade-lodash
```

Queries the GitLab API for each MR and reports whether it is open, merged, or closed.

### list

List all batch change specs.

```bash
batch-changes list
```

### health

Verify connectivity to Sourcegraph, GitLab, and Vault.

```bash
batch-changes health
```

## How It Works

1. **create** — Saves a JSON spec with the search query, script, branch name, and commit message.
2. **run** — Queries Sourcegraph's GraphQL API to find matching repos. For each repo:
   - Clones via SSH (`ssh://git@192.168.1.10:2222/{namespace}/{project}.git`)
   - Creates a branch, runs the script, stages and commits changes
   - Pushes the branch and opens a GitLab MR via `POST /api/v4/projects/:id/merge_requests`
   - Records the result (success, failure reason, MR URL) in a results file
3. **status** — Reads the results file and checks each MR's current state via the GitLab API.

## File Locations

| Path | Purpose |
|---|---|
| `~/.toolshed/data/batch-changes/{name}.json` | Batch change spec |
| `~/.toolshed/data/batch-changes/{name}-results.json` | Execution results with MR info |
| `/tmp/batch-changes/{name}/` | Temporary clone directory (cleaned up after run) |
