#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root/infrastructure/production"
test -f .env || { echo 'Create infrastructure/production/.env first' >&2; exit 1; }
chmod 600 .env
docker compose config --quiet
# Build replacement images before modifying running containers.
docker compose build api web
docker compose up -d --wait postgres
docker compose run --rm --no-deps api node dist/migrate.js
docker compose up -d --wait --wait-timeout 120 api web
docker compose exec -T api node -e "fetch('http://127.0.0.1:4000/ready').then(r=>{if(!r.ok)process.exit(1)})"
docker compose exec -T web node -e "fetch('http://127.0.0.1:3000/health').then(r=>{if(!r.ok)process.exit(1)})"
echo 'SafeEat services healthy'
