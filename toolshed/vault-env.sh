#!/bin/sh
set -e
# Resolve secrets from Vault and export as env vars before starting the app.
# Requires VAULT_ADDR and VAULT_TOKEN to be set.

vault_get() {
  # vault_get <path> <key> — returns the value or empty string
  curl -sf -H "X-Vault-Token: $VAULT_TOKEN" \
    "${VAULT_ADDR}/v1/secret/data/$1" 2>/dev/null \
    | sed -n "s/.*\"$2\":\"\\([^\"]*\\)\".*/\\1/p"
}

if [ -z "$VAULT_ADDR" ] || [ -z "$VAULT_TOKEN" ]; then
  echo "vault-env: VAULT_ADDR or VAULT_TOKEN not set, skipping secret resolution" >&2
  exec "$@"
fi

echo "vault-env: resolving secrets from ${VAULT_ADDR}" >&2

# GitLab
v=$(vault_get gitlab token); [ -n "$v" ] && export GITLAB_TOKEN="$v"

# Sourcegraph
v=$(vault_get sourcegraph token); [ -n "$v" ] && export SRC_ACCESS_TOKEN="$v"

# OpenRouter
v=$(vault_get openrouter api_key); [ -n "$v" ] && export OPENROUTER_API_KEY="$v"

# DeepInfra
v=$(vault_get deepinfra value); [ -n "$v" ] && export DEEPINFRA_API_KEY="$v"

# Brave
v=$(vault_get brave api_key); [ -n "$v" ] && export BRAVE_API_KEY="$v"

# Unleash
v=$(vault_get unleash api_token); [ -n "$v" ] && export UNLEASH_API_TOKEN="$v"

# E2B
v=$(vault_get e2b api_key); [ -n "$v" ] && export E2B_API_KEY="$v"

# Huly
v=$(vault_get huly/credentials HULY_EMAIL); [ -n "$v" ] && export HULY_EMAIL="$v"
v=$(vault_get huly/credentials HULY_PASSWORD); [ -n "$v" ] && export HULY_PASSWORD="$v"
v=$(vault_get huly/credentials HULY_WORKSPACE); [ -n "$v" ] && export HULY_WORKSPACE="$v"

# Langfuse
v=$(vault_get langfuse public_key); [ -n "$v" ] && export LANGFUSE_PUBLIC_KEY="$v"
v=$(vault_get langfuse secret_key); [ -n "$v" ] && export LANGFUSE_SECRET_KEY="$v"

# GitLab Runner
v=$(vault_get gitlab runner_token); [ -n "$v" ] && export RUNNER_AUTH_TOKEN="$v"

echo "vault-env: secrets resolved, starting application" >&2
exec "$@"
