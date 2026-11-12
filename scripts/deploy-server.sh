#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root/infrastructure/production"
export SAFEEAT_ENV_FILE="$(realpath "${1:-.env}")"
test -f "$SAFEEAT_ENV_FILE" || { echo 'Create the environment file first' >&2; exit 1; }
chmod 600 "$SAFEEAT_ENV_FILE"
compose=(docker compose --env-file "$SAFEEAT_ENV_FILE")
"${compose[@]}" config --quiet
# Build the replacement image before modifying running containers, unless deploy-remote.sh
# already loaded a prebuilt one (SAFEEAT_SKIP_BUILD=1). api and web share the same image.
if [[ "${SAFEEAT_SKIP_BUILD:-0}" != 1 ]]; then "${compose[@]}" build api; fi
"${compose[@]}" up -d --wait postgres
# Migrations are additive, so the previous image keeps working if a rollback is needed.
"${compose[@]}" run --rm --no-deps api migrate
# Optional on-server model for "Verificado por IA" (see docker-compose.yml, service ai).
if grep -q '^AI_LOCAL_URL=' "$SAFEEAT_ENV_FILE"; then
  "${compose[@]}" --profile ai up -d --wait --wait-timeout 180 ai
fi
"${compose[@]}" up -d --wait --wait-timeout 120 api web
"${compose[@]}" exec -T api safeeat-server healthcheck http://127.0.0.1:4000/ready
"${compose[@]}" exec -T web sh -c 'exec safeeat-server healthcheck "http://127.0.0.1:3000${WEB_BASE_PATH:-}/health"'
echo 'SafeEat services healthy'
