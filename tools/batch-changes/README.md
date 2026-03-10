# batch-changes

Multi-repo change automation via Sourcegraph search + GitLab API.

Command names align with the Sourcegraph `src batch` CLI.

## Commands

| Command        | Description                                                        |
| -------------- | ------------------------------------------------------------------ |
| `new`          | Create a new batch change spec (saved locally, not yet executed)   |
| `apply`        | Execute a batch change: clone repos, run script, push, open MRs   |
| `preview`      | Dry-run — show which repos match and what would happen             |
| `validate`     | Check that a spec exists and is well-formed                        |
| `repositories` | List repos that would be affected without cloning or changing anything |
| `status`       | Show MR status for an executed batch change                        |
| `list`         | List all batch change specs                                        |
| `health`       | Check connectivity to Sourcegraph, GitLab, and Vault               |

## Quick start

```bash
# Create a spec
batch-changes new \
  --name fix-typo \
  --query 'repo:myorg/ file:README.md teh' \
  --script "sed -i 's/teh/the/g' README.md"

# Validate it
batch-changes validate fix-typo

# See which repos match
batch-changes repositories fix-typo

# Preview what apply would do
batch-changes preview fix-typo

# Execute for real
batch-changes apply fix-typo

# Check MR status
batch-changes status fix-typo
```

## Migration from previous command names

| Old command | New command    |
| ----------- | -------------- |
| `create`    | `new`          |
| `run`       | `apply`        |
| `status`    | `status`       |
| `list`      | `list`         |
| `health`    | `health`       |
| (none)      | `preview`      |
| (none)      | `validate`     |
| (none)      | `repositories` |
