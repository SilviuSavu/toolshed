#!/bin/sh
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
IMAGE_TAG="rc-server"

GREEN='\033[0;32m'
RED='\033[0;31m'
BOLD='\033[1m'
RESET='\033[0m'

log()  { printf "${BOLD}[build]${RESET} %s\n" "$1"; }
ok()   { printf "${GREEN}  ✓${RESET} %s\n" "$1"; }
fail() { printf "${RED}  ✗${RESET} %s\n" "$1" >&2; exit 1; }

RC_SRC="$HOME/rc"
RC_DST="$SCRIPT_DIR/rc"

if [ ! -d "$RC_SRC" ]; then
  fail "rc source not found at $RC_SRC"
fi

log "Copying rc_server code..."
rm -rf "$RC_DST" 2>/dev/null || true
mkdir -p "$RC_DST"
cp "$RC_SRC/rc_server.py" "$RC_DST/"
cp -r "$RC_SRC/review_context" "$RC_DST/"
find "$RC_DST" -type d -name "__pycache__" -exec rm -rf {} + 2>/dev/null || true
find "$RC_DST" -name ".DS_Store" -delete 2>/dev/null || true
ok "rc_server code copied"

log "Building image: $IMAGE_TAG"
docker build \
  --platform linux/amd64 \
  -t "$IMAGE_TAG" \
  -f "$SCRIPT_DIR/Dockerfile" \
  "$SCRIPT_DIR"

ok "Image built: $IMAGE_TAG"
SIZE=$(docker image inspect "$IMAGE_TAG" --format '{{.Size}}' | awk '{printf "%.0fMB", $1/1024/1024}')
log "Image size: $SIZE"
