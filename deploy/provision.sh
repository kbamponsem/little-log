#!/usr/bin/env bash
#
# Runs INSIDE the Little Log LXC. Terraform uploads this file along with the
# source and executes it over SSH, so the only way to "run" it by hand is:
#
#   ssh root@<container> 'bash /opt/little-log/app/deploy/provision.sh'
#
# Idempotent: safe to re-run, and it never regenerates SESSION_SECRET or
# touches the data directory.

set -euo pipefail

APP_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DATA_DIR="$APP_ROOT/data"

echo "==> Docker"
if command -v docker >/dev/null 2>&1; then
  echo "    already installed"
else
  export DEBIAN_FRONTEND=noninteractive
  apt-get update
  apt-get install -y --no-install-recommends \
    ca-certificates curl openssl gnupg fuse-overlayfs

  # Docker's own repo, so we get the Compose v2 plugin.
  install -m 0755 -d /etc/apt/keyrings
  curl -fsSL https://download.docker.com/linux/debian/gpg -o /etc/apt/keyrings/docker.asc
  chmod a+r /etc/apt/keyrings/docker.asc
  echo "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/docker.asc] \
https://download.docker.com/linux/debian $(. /etc/os-release && echo "$VERSION_CODENAME") stable" \
    > /etc/apt/sources.list.d/docker.list
  apt-get update
  apt-get install -y --no-install-recommends \
    docker-ce docker-ce-cli containerd.io docker-compose-plugin

  # overlay2, not fuse-overlayfs: fuse-overlayfs needs /dev/fuse, which comes
  # from the container's `fuse` feature flag — and that flag requires root@pam,
  # which this deployment deliberately does not use. If Docker refuses to start
  # with "not supported as upperdir" (seen on some ZFS-backed storages),
  # switch this to "vfs": slower and hungrier for disk, but works anywhere.
  mkdir -p /etc/docker
  cat > /etc/docker/daemon.json <<'JSON'
{
  "storage-driver": "overlay2",
  "log-driver": "json-file",
  "log-opts": { "max-size": "10m", "max-file": "3" }
}
JSON
fi

systemctl enable --now docker

echo "==> Configuration"
mkdir -p "$DATA_DIR"

# The secret signs session cookies. Regenerating it signs every device out, so
# only ever create it once.
if [ -f "$APP_ROOT/.env" ]; then
  echo "    $APP_ROOT/.env kept (SESSION_SECRET unchanged)"
else
  cat > "$APP_ROOT/.env" <<ENV
SESSION_SECRET=$(openssl rand -hex 32)
PORT=8080
COOKIE_SECURE=0
RUST_LOG=info
ENV
  chmod 600 "$APP_ROOT/.env"
  echo "    wrote $APP_ROOT/.env with a new SESSION_SECRET"
fi

echo "==> Building the image (first run compiles Rust — this is the slow part)"
cd "$APP_ROOT"
docker compose up -d --build

echo "==> Waiting for the service"
for _ in $(seq 1 60); do
  if curl -fsS http://localhost:8080/healthz >/dev/null 2>&1; then
    IP="$(hostname -I 2>/dev/null | awk '{print $1}')"
    echo
    echo "Little Log is up on http://${IP:-localhost}:8080"
    echo "The first account you create becomes the owner."
    exit 0
  fi
  sleep 2
done

echo "!! The service did not answer /healthz within 2 minutes." >&2
docker compose logs --tail 40 >&2 || true
exit 1
