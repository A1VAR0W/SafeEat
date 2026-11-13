#!/usr/bin/env bash
set -euo pipefail
umask 077
root="$(cd "$(dirname "$0")/.." && pwd)"
destination="${SAFEEAT_BACKUP_DIR:-/opt/safeeat/backups/postgres}"
mkdir -p "$destination"
cd "$root/infrastructure/production"
file="$destination/safeeat-$(date -u +%Y%m%dT%H%M%SZ).dump"
docker compose exec -T postgres pg_dump -U safeeat -d safeeat -Fc > "$file.partial"
test -s "$file.partial"
docker compose exec -T postgres pg_restore --list < "$file.partial" > /dev/null
mv "$file.partial" "$file"
# --list only reads the archive's table of contents. With SAFEEAT_VERIFY_RESTORE=1 the
# dump is also restored into a throwaway container and checked (docs/backup-recovery.md).
if [[ "${SAFEEAT_VERIFY_RESTORE:-0}" == 1 ]]; then
  bash "$root/scripts/verify-backup-restore.sh" "$file"
fi
find "$destination" -maxdepth 1 -type f -name 'safeeat-*.dump' -mtime +14 -delete
echo "Verified backup: $file"
