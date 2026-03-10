# code-monitor

A toolshed native tool that watches for new commits matching Sourcegraph search queries and sends alerts via webhooks.

## Overview

code-monitor creates persistent monitors that track Sourcegraph search queries (typically `type:diff` or `type:commit` searches). When new results appear, it records them and optionally fires a webhook (e.g., to a Slack incoming webhook URL).

State is stored in a local SQLite database at `~/.toolshed/data/code-monitor.db`.

## Prerequisites

- **Sourcegraph instance** with API access
- **Vault** running with Sourcegraph credentials stored at `secret/data/sourcegraph` (keys: `url`, `token`)
- **sqlite3** available on PATH
- **jq** available on PATH
- **curl** available on PATH

## Configuration

Credentials are loaded from Vault at runtime:

| Variable      | Default                     | Description          |
|---------------|-----------------------------|----------------------|
| `VAULT_ADDR`  | `http://127.0.0.1:8200`    | Vault server address |
| `VAULT_TOKEN` | `toolshed-dev-token`        | Vault access token   |

The Vault secret at `secret/data/sourcegraph` must contain:

```json
{
  "url": "https://sourcegraph.example.com",
  "token": "sgp_xxxxxxxxxxxx"
}
```

## Commands

### create

Create a new monitor.

```bash
code-monitor create --name "security-deps" \
  --query "type:commit repo:myorg/ file:package-lock.json" \
  --webhook "https://hooks.slack.com/services/T.../B.../xxx" \
  --interval 30
```

| Flag         | Required | Description                                    |
|--------------|----------|------------------------------------------------|
| `--name`     | Yes      | Unique monitor name                            |
| `--query`    | Yes      | Sourcegraph search query                       |
| `--webhook`  | No       | Webhook URL for alert delivery                 |
| `--interval` | No       | Check interval in minutes (default: 15, informational only) |

### check

Run a check for one or all monitors.

```bash
code-monitor check --name "security-deps"
code-monitor check --name "security-deps" --since "2026-03-09T00:00:00Z"
code-monitor check  # checks all monitors
```

| Flag      | Required | Description                                          |
|-----------|----------|------------------------------------------------------|
| `--name`  | No       | Specific monitor (omit to check all)                 |
| `--since` | No       | Override the "since" timestamp (ISO8601)             |

### check-all

Run checks for every active monitor. Equivalent to `check` with no `--name`.

```bash
code-monitor check-all
```

### list

List all monitors with their status.

```bash
code-monitor list
```

Output columns: name, query, last checked, last alert count, interval, webhook configured (y/n).

### delete

Delete a monitor and its alert history.

```bash
code-monitor delete security-deps
```

### history

Show recent alerts for a monitor.

```bash
code-monitor history security-deps
code-monitor history security-deps --limit 5
```

| Flag      | Required | Description                     |
|-----------|----------|---------------------------------|
| `--limit` | No       | Max results to show (default: 20) |

### health

Check Sourcegraph connectivity and database state.

```bash
code-monitor health
```

## Webhook Payload Format

When a monitor fires, the following JSON is POSTed to the configured webhook:

```json
{
  "monitor": "security-deps",
  "query": "type:commit repo:myorg/ file:package-lock.json",
  "match_count": 5,
  "matches": [
    {
      "repo": "myorg/frontend",
      "file": "package-lock.json",
      "line": 42,
      "preview": "Updated lodash from 4.17.20 to 4.17.21"
    }
  ],
  "checked_at": "2026-03-10T12:00:00Z"
}
```

## Database Schema

Two tables in `~/.toolshed/data/code-monitor.db`:

- **monitors** — name (PK), query, webhook, interval_min, last_checked, created_at
- **alerts** — id (PK), monitor_name (FK), match_count, matches (JSON), fired_at

## Scheduling

The `--interval` flag is informational only. Actual periodic execution should be handled externally (e.g., cron, systemd timer, or a toolshed scheduler):

```cron
*/15 * * * * /path/to/code-monitor check-all
```
