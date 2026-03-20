#!/bin/sh
set -e
# Wait for GitLab readiness, register runner if needed, then start.

until curl -sf http://gitlab/users/sign_in >/dev/null 2>&1; do
  echo "Waiting for GitLab..."; sleep 5
done

# Register if not already configured
if [ ! -f /etc/gitlab-runner/config.toml ] || ! grep -q 'url' /etc/gitlab-runner/config.toml; then
  gitlab-runner register \
    --non-interactive \
    --url "http://gitlab" \
    --token "${RUNNER_AUTH_TOKEN}" \
    --executor "docker" \
    --docker-image "alpine:latest" \
    --docker-network-mode "infra" \
    --description "e2e-runner"
fi

exec gitlab-runner run
