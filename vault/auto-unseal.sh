#!/bin/sh
# Vault auto-unseal script for launchd.
# Polls Vault health endpoint and unseals when sealed.
# Used by com.orchestrator.vault-unseal.plist on macOS login/reboot.
set -e

VAULT_ADDR="http://127.0.0.1:8200"
MAX_ATTEMPTS=60  # 300s at 5s intervals — Docker Desktop may take a while after reboot

log() { echo "[vault-unseal] $(date '+%Y-%m-%d %H:%M:%S') $1"; }

UNSEAL_KEY=$(security find-generic-password -a vault -s vault-unseal-key -w 2>/dev/null)
if [ -z "$UNSEAL_KEY" ]; then
  log "ERROR: could not read vault-unseal-key from Keychain"
  exit 1
fi

attempt=0
while [ "$attempt" -lt "$MAX_ATTEMPTS" ]; do
  attempt=$((attempt + 1))
  health=$(curl -s "$VAULT_ADDR/v1/sys/health" 2>/dev/null) || true

  if [ -z "$health" ]; then
    log "vault not reachable (attempt $attempt/$MAX_ATTEMPTS)"
    sleep 5
    continue
  fi

  if echo "$health" | grep -q '"sealed":false'; then
    log "vault already unsealed"
    exit 0
  fi

  if echo "$health" | grep -q '"sealed":true'; then
    log "vault is sealed, unsealing..."
    response=$(curl -s -X POST "$VAULT_ADDR/v1/sys/unseal" -d "{\"key\":\"$UNSEAL_KEY\"}")
    if echo "$response" | grep -q '"errors"'; then
      log "ERROR: unseal failed — $response"
      exit 1
    fi
    if echo "$response" | grep -q '"sealed":false'; then
      log "vault unsealed successfully"
      exit 0
    fi
    log "unseal submitted, vault still sealed (multi-key?)"
  fi

  sleep 5
done

log "ERROR: vault did not become ready within $((MAX_ATTEMPTS * 5))s"
exit 1
