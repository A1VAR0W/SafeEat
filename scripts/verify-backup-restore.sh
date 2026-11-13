#!/usr/bin/env bash
# Proves that a SafeEat PostgreSQL backup can be restored, instead of only listing it.
#
#   bash scripts/verify-backup-restore.sh /opt/safeeat/backups/postgres/safeeat-<date>.dump
#
# 1. Starts a throwaway PostGIS container: no published port, its own network, CPU and
#    memory limits, removed on exit. Production and preproduction are never touched.
# 2. Restores the dump into it (pg_restore --exit-on-error, single transaction).
# 3. Checks: every migration in db/migrations is recorded; core tables exist and are
#    readable; foreign keys hold (each constraint is re-validated); row counts printed.
# 4. Removes the container and its network whatever happens.
#
# Exit status 0 only if every check passed. Run it after scripts/backup-postgres.sh
# (or weekly from a timer). See docs/backup-recovery.md.
set -euo pipefail

dump="${1:?Usage: verify-backup-restore.sh <file.dump>}"
[[ -s "$dump" ]] || { echo "Backup not found or empty: $dump" >&2; exit 1; }
root="$(cd "$(dirname "$0")/.." && pwd)"
image="${SAFEEAT_VERIFY_IMAGE:-postgis/postgis:17-3.5-alpine}"
name="safeeat-restore-check-$$"
network="$name-net"

cleanup() {
  docker rm -f "$name" > /dev/null 2>&1 || true
  docker network rm "$network" > /dev/null 2>&1 || true
}
trap cleanup EXIT

docker network create --internal "$network" > /dev/null
docker run -d --name "$name" --network "$network" \
  --cpus 1 --memory 1g \
  -e POSTGRES_USER=restorecheck -e POSTGRES_PASSWORD="$(head -c 18 /dev/urandom | base64 | tr -d '/+=')" \
  -e POSTGRES_DB=restorecheck \
  "$image" > /dev/null

for _ in $(seq 1 60); do
  docker exec "$name" pg_isready -U restorecheck -d restorecheck > /dev/null 2>&1 && break
  sleep 1
done
docker exec "$name" pg_isready -U restorecheck -d restorecheck > /dev/null

psql() { docker exec -i "$name" psql -v ON_ERROR_STOP=1 -qtA -U restorecheck -d restorecheck "$@"; }

# The dump owner may not exist here: restore without owners or grants.
docker exec -i "$name" pg_restore --exit-on-error --single-transaction --no-owner --no-acl \
  -U restorecheck -d restorecheck < "$dump"
echo "Restore: OK"

failed=0
check() { echo "$1: $2"; [[ "$2" == OK* ]] || failed=1; }

# Every migration shipped in the repository is recorded in the backup.
missing=""
for file in "$root"/db/migrations/*.sql; do
  migration="$(basename "$file")"
  found="$(psql -c "SELECT count(*) FROM schema_migrations WHERE name='$migration'")"
  [[ "$found" == 1 ]] || missing="$missing $migration"
done
if [[ -z "$missing" ]]; then
  check "Migrations" "OK"
else
  # A backup taken before a deploy legitimately lacks the newest migrations.
  check "Migrations" "MISSING$missing (fine only if the backup predates them)"
fi

# Core tables exist and can be read.
for table in users refresh_sessions products user_product_verdicts favorite_products \
  saved_collections product_gluten_reports restaurant_catalog restaurant_gluten_reports; do
  if count="$(psql -c "SELECT count(*) FROM $table" 2> /dev/null)"; then
    echo "  $table: $count rows"
  else
    check "Table $table" "UNREADABLE"
  fi
done

# Foreign keys: re-validate every constraint against the restored data.
invalid=0
while IFS='|' read -r table constraint; do
  [[ -n "$table" ]] || continue
  if ! psql -c "ALTER TABLE $table VALIDATE CONSTRAINT \"$constraint\"" > /dev/null 2>&1; then
    echo "  invalid: $table.$constraint"
    invalid=1
  fi
done < <(psql -c "SELECT conrelid::regclass, conname FROM pg_constraint WHERE contype='f' AND connamespace='public'::regnamespace")
# VALIDATE only re-checks NOT VALID constraints; force a real check on the rest.
orphans="$(psql -c "
  SELECT count(*) FROM (
    SELECT 1 FROM refresh_sessions s LEFT JOIN users u ON u.id=s.user_id WHERE u.id IS NULL
    UNION ALL SELECT 1 FROM user_product_verdicts v LEFT JOIN users u ON u.id=v.user_id WHERE u.id IS NULL
    UNION ALL SELECT 1 FROM favorite_products f LEFT JOIN users u ON u.id=f.user_id WHERE u.id IS NULL
  ) o")"
if [[ "$invalid" == 0 && "$orphans" == 0 ]]; then
  check "Integrity" "OK"
else
  check "Integrity" "FAILED (invalid constraints: $invalid, orphan rows: $orphans)"
fi

if [[ "$failed" == 0 ]]; then
  echo "Backup verified by restore: $dump"
else
  echo "Backup restore check FAILED: $dump" >&2
  exit 1
fi
