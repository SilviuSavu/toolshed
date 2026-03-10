# code-insights

Track Sourcegraph search result counts over time to visualize code trends.

## Overview

`code-insights` is a toolshed native tool that periodically records how many
results a Sourcegraph search query returns.  Over time this builds a time-series
you can visualise as an ASCII trend chart, compare across metrics, or export as
JSON for dashboards.

Typical use cases:

- Track deprecated API usage across a codebase and verify it decreases over time.
- Monitor adoption of a new library or pattern.
- Compare two competing approaches (e.g., old ORM vs new ORM).

## Prerequisites

| Dependency | Purpose |
|---|---|
| `sqlite3` | Local state storage |
| `curl` | Vault + Sourcegraph API calls |
| `jq` | JSON processing |
| HashiCorp Vault | Credential storage for Sourcegraph URL and token |

Sourcegraph credentials are stored in Vault at `secret/data/sourcegraph` with
keys `url` and `token`.

## Configuration

| Variable | Default | Description |
|---|---|---|
| `VAULT_ADDR` | `http://127.0.0.1:8200` | Vault server address |
| `VAULT_TOKEN` | `toolshed-dev-token` | Vault authentication token |

Data is stored in `~/.toolshed/data/code-insights.db` (SQLite).

## Commands

### create

Create a new insight (tracked metric).

```bash
run create --name deprecated-api-usage \
           --query "lang:go fmt.Println" \
           --description "Track leftover debug prints" \
           --interval 24
```

### collect

Run a collection for one or all insights.  Executes the Sourcegraph search
query and records the match count plus per-repository breakdown.

```bash
run collect --name deprecated-api-usage   # one insight
run collect                                # all insights
```

### collect-all

Shorthand to collect data for every defined insight.

```bash
run collect-all
```

### trend

Display a time-series trend with an ASCII bar chart.

```bash
run trend deprecated-api-usage --days 30 --format text
```

Example output:

```
deprecated-api-usage (last 30 days)
Query: lang:go fmt.Println

Date        Count  Delta
----        -----  -----
2026-03-10    142         ██████████████
2026-03-09    138    -4   █████████████
2026-03-08    145    +7   ███████████████
```

Use `--format json` for machine-readable output.

### compare

Show two insights side-by-side, aligned by date.

```bash
run compare deprecated-api-usage new-logger-adoption --days 30
```

### snapshot

Query all insights right now and display current counts without storing data.

```bash
run snapshot
```

### list

List all defined insights with data-point counts and last-collected timestamps.

```bash
run list
```

### delete

Remove an insight and all its collected data.

```bash
run delete deprecated-api-usage
```

### health

Verify Sourcegraph connectivity, Vault credentials, and database state.

```bash
run health
```

## Database Schema

```sql
CREATE TABLE insights (
    name TEXT PRIMARY KEY,
    query TEXT NOT NULL,
    description TEXT,
    interval_hours INTEGER DEFAULT 24,
    created_at TEXT DEFAULT (datetime('now'))
);

CREATE TABLE data_points (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    insight_name TEXT NOT NULL,
    match_count INTEGER NOT NULL,
    by_repo TEXT,  -- JSON: {"repo1": count1, "repo2": count2}
    collected_at TEXT DEFAULT (datetime('now')),
    FOREIGN KEY (insight_name) REFERENCES insights(name)
);
```

## Scheduling

The `--interval` parameter on `create` is informational only.  To collect data
on a schedule, use cron or a similar scheduler:

```cron
0 */6 * * *  /path/to/code-insights/run collect-all
```
