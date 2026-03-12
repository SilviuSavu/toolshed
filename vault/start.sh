#!/bin/sh
set -e

DIR="$(cd "$(dirname "$0")/.." && pwd)"

# Read secrets from macOS Keychain
UNSEAL_KEY=$(security find-generic-password -a vault -s vault-unseal-key -w)
VAULT_TOKEN=$(security find-generic-password -a vault -s vault-toolshed-token -w)
export VAULT_TOKEN

# Start vault
docker compose -f "$DIR/vault/docker-compose.yml" up -d
echo "waiting for vault..."
sleep 3

# Unseal from host
curl -s -X POST http://127.0.0.1:8200/v1/sys/unseal \
  -d "{\"key\":\"$UNSEAL_KEY\"}" > /dev/null
echo "vault: $(curl -s http://127.0.0.1:8200/v1/sys/health | grep -o '"sealed":false')"

# Start remaining services
docker compose -f "$DIR/toolshed/docker-compose.yml" up -d
docker compose -f "$DIR/gitlab/docker-compose.yml" up -d
docker compose -f "$DIR/sourcegraph/docker-compose.yml" up -d
docker compose -f "$DIR/huly/docker-compose.yml" up -d

echo "all services starting"
