#!/bin/sh
set -e

# Start vault server in background
vault server -config=/vault/config/config.hcl &
VAULT_PID=$!

# Wait for vault to be reachable
for i in $(seq 1 30); do
  if vault status -format=json 2>/dev/null | grep -q '"initialized"'; then
    break
  fi
  sleep 1
done

# Auto-unseal if sealed and key is provided
if [ -n "$VAULT_UNSEAL_KEY" ]; then
  SEALED=$(vault status -format=json 2>/dev/null | grep '"sealed"' | grep -c 'true' || true)
  if [ "$SEALED" = "1" ]; then
    vault operator unseal "$VAULT_UNSEAL_KEY" > /dev/null 2>&1
  fi
fi

# Wait for vault process
wait $VAULT_PID
