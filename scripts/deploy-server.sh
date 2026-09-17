#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root/infrastructure/production"
export SAFEEAT_ENV_FILE="$(realpath "${1:-.env}")"
test -f "$SAFEEAT_ENV_FILE" || { echo 'Create the environment file first' >&2; exit 1; }
chmod 600 "$SAFEEAT_ENV_FILE"
compose=(docker compose --env-file "$SAFEEAT_ENV_FILE")
"${compose[@]}" config --quiet
# Build replacement images before modifying running containers.
"${compose[@]}" build api web
"${compose[@]}" up -d --wait postgres
"${compose[@]}" run --rm --no-deps api node dist/db/migrate.js
"${compose[@]}" up -d --wait --wait-timeout 120 api web
"${compose[@]}" exec -T api node -e "fetch('http://127.0.0.1:4000/ready').then(r=>{if(!r.ok)process.exit(1)})"
"${compose[@]}" exec -T web node -e "fetch('http://127.0.0.1:3000'+(process.env.WEB_BASE_PATH||'')+'/health').then(r=>{if(!r.ok)process.exit(1)})"
echo 'SafeEat services healthy'
