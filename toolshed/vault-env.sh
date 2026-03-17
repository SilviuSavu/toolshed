#!/bin/sh
set -e
# Resolve secrets from Vault and export as env vars before starting the app.
# Requires VAULT_ADDR and VAULT_TOKEN to be set.

vault_get() {
  # vault_get <path> <key> — returns the value or empty string
  curl -sf -H "X-Vault-Token: $VAULT_TOKEN" \
    "${VAULT_ADDR}/v1/secret/data/$1" 2>/dev/null \
    | jq -r ".data.data.$2 // empty"
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
v=$(vault_get sourcegraph url);   [ -n "$v" ] && export SRC_ENDPOINT="$v"

# Unleash
v=$(vault_get unleash api_token); [ -n "$v" ] && export UNLEASH_API_TOKEN="$v"

# Huly
v=$(vault_get huly/credentials HULY_EMAIL);     [ -n "$v" ] && export HULY_EMAIL="$v"
v=$(vault_get huly/credentials HULY_PASSWORD);   [ -n "$v" ] && export HULY_PASSWORD="$v"
v=$(vault_get huly/credentials HULY_WORKSPACE);  [ -n "$v" ] && export HULY_WORKSPACE="$v"

echo "vault-env: secrets resolved, starting application" >&2
exec "$@"
