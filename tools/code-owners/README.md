# code-owners

A Toolshed native tool that parses CODEOWNERS files from Sourcegraph-indexed repositories and resolves file ownership.

## Overview

`code-owners` syncs CODEOWNERS rules from your repositories into a local SQLite database, then lets you query ownership, search by owner, and find files with no coverage — all without cloning repos locally.

## Prerequisites

- **Sourcegraph** instance with API access (credentials stored in Vault)
- **Vault** for credential management
- `curl`, `jq`, `sqlite3` available on PATH

## Setup

Sourcegraph credentials are read from Vault at `secret/data/sourcegraph`:

```bash
vault kv put secret/sourcegraph \
  url="https://sourcegraph.example.com" \
  token="sgp_xxxxxxxxxxxx"
```

Environment variables (with defaults):

| Variable      | Default                     |
|---------------|-----------------------------|
| `VAULT_ADDR`  | `http://127.0.0.1:8200`    |
| `VAULT_TOKEN` | `toolshed-dev-token`        |

## Commands

### sync

Fetch and parse CODEOWNERS files from all indexed repos (or a specific one).

```bash
code-owners sync
code-owners sync --repo github.com/org/repo
```

Looks for CODEOWNERS in standard locations: `CODEOWNERS`, `.github/CODEOWNERS`, `.gitlab/CODEOWNERS`, `docs/CODEOWNERS`.

### query

Find who owns a specific file.

```bash
code-owners query github.com/org/repo src/api/handler.go
```

Applies CODEOWNERS rules in order; the last matching rule wins (per the CODEOWNERS spec).

### search

Find all patterns and repos owned by a person or team.

```bash
code-owners search @platform-team
code-owners search @jdoe --repo github.com/org/repo
```

### list-repos

List all repos that have CODEOWNERS files, with rule counts and last-synced times.

```bash
code-owners list-repos
```

### list-owners

List all known owners with the number of rules each owns.

```bash
code-owners list-owners
code-owners list-owners --repo github.com/org/repo
```

### unowned

Find files in a repo that have no CODEOWNERS coverage.

```bash
code-owners unowned github.com/org/repo
```

### health

Check Sourcegraph connectivity and database state.

```bash
code-owners health
```

## Data Storage

Rules are stored in SQLite at `~/.toolshed/data/code-owners.db` with two tables:

- **repos** — tracked repositories, their CODEOWNERS path, and last sync time
- **rules** — individual CODEOWNERS rules (pattern, owners, line number) per repo

## CODEOWNERS Format

The parser follows the standard CODEOWNERS format:

- Lines starting with `#` are comments
- Empty lines are ignored
- Each rule: `<glob-pattern> @owner1 @owner2 ...`
- Patterns use gitignore-style globs (`*`, `**`, `/path`, `path/`)
- Last matching pattern wins

## Pattern Matching

The tool implements simplified gitignore-style glob matching:

| Pattern       | Matches                                        |
|---------------|------------------------------------------------|
| `*.js`        | Any `.js` file in any directory                |
| `/src/`       | Everything under `src/` at the repo root       |
| `src/**/*.go` | Any `.go` file nested under `src/`             |
| `docs/*`      | Files directly in any `docs/` directory        |
