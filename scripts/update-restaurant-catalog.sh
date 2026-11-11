#!/usr/bin/env bash
set -euo pipefail

# Requires osmium-tool and curl (and cargo when run locally). Pass a compose env file on the server, or set DATABASE_URL locally.
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
region="${1:-madrid}"
compose_env="${2:-}"
case "$region" in
  madrid) url='https://download.geofabrik.de/europe/spain/madrid-latest.osm.pbf' ;;
  spain) url='https://download.geofabrik.de/europe/spain-latest.osm.pbf' ;;
  *) echo 'Use madrid or spain' >&2; exit 2 ;;
esac
if [[ -z "$compose_env" ]]; then : "${DATABASE_URL:?DATABASE_URL or a compose env file is required}"; fi
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
curl --fail --location --retry 3 --output "$work/source.osm.pbf" "$url"
osmium tags-filter "$work/source.osm.pbf" \
  'nwr/amenity=restaurant,cafe,fast_food,bar,pub,food_court,biergarten' \
  'nwr/shop=bakery,pastry,convenience' \
  --output "$work/food.osm.pbf"
osmium export "$work/food.osm.pbf" -f geojsonseq -a type,id \
  --output "$work/food.geojsonseq"
if [[ -n "$compose_env" ]]; then
  export SAFEEAT_ENV_FILE="$(realpath "$compose_env")"
  docker compose -f infrastructure/production/docker-compose.yml \
    --env-file "$SAFEEAT_ENV_FILE" run --rm --no-deps \
    -v "$work:/import:ro" api import-restaurants "$region" /import/food.geojsonseq
else
  cargo run --release -q -p safeeat-server -- import-restaurants "$region" "$work/food.geojsonseq"
fi
