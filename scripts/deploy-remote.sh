#!/usr/bin/env bash
# Deploys SafeEat to the home server over SSH (the host it shares with HallowFest).
#
#   scripts/deploy-remote.sh [--env preproduction|production] [--host chatink-dev] [--build-on-server] [--yes]
#
# 1. Builds the Rust image locally (keeps the shared server's CPU free) and streams it over SSH.
#    --build-on-server sends only the sources and builds there (when local Docker is unavailable).
# 2. Ships the deploy files to /opt/safeeat/releases/<tag>; secrets stay in the server's env files.
# 3. Production only: verified pg_dump backup first.
# 4. Runs scripts/deploy-server.sh (additive migrations, api + web, health checks).
# 5. Checks /api/v1/meta through the Tailscale Funnel; on any failure restores the previous containers.
#
# Never touches the Funnel config, nginx, HallowFest's ports (443, 3307, 3308, 3399) or the volumes.
set -euo pipefail

environment=preproduction
host="${SAFEEAT_SSH_HOST:-chatink-dev}"
confirmed=0
build_on_server=0
while [[ $# -gt 0 ]]; do
  case "$1" in
    --env) environment="$2"; shift 2 ;;
    --host) host="$2"; shift 2 ;;
    --yes) confirmed=1; shift ;;
    --build-on-server) build_on_server=1; shift ;;
    *) echo "Unknown argument: $1" >&2; exit 2 ;;
  esac
done
case "$environment" in
  preproduction) env_file=preproduction.env; prefix=/preproduction; project=safeeat-preproduction ;;
  production) env_file=.env; prefix=; project=safeeat ;;
  *) echo 'Use --env preproduction or --env production' >&2; exit 2 ;;
esac

remote_root=/opt/safeeat
public_origin="${SAFEEAT_PUBLIC_ORIGIN:-https://chat-ink.tail552c89.ts.net:10000}"
hallowfest_health="${HALLOWFEST_HEALTH_URL:-https://chat-ink.tail552c89.ts.net/health}"
root="$(git -C "$(dirname "$0")" rev-parse --show-toplevel)"
cd "$root"

tag="$(git rev-parse --short=12 HEAD)"
if [[ -n "$(git status --porcelain)" ]]; then tag="$tag-dirty-$(date -u +%Y%m%d%H%M%S)"; fi
version="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
log() { printf '\n==> %s\n' "$*"; }
remote() { ssh -o BatchMode=yes -o ConnectTimeout=10 "$host" "$@"; }

if [[ "$environment" == production && "$confirmed" != 1 ]]; then
  read -r -p "Deploy $tag (v$version) to PRODUCTION on $host? Type 'production': " answer
  [[ "$answer" == production ]] || { echo 'Cancelled'; exit 1; }
fi

log "Preflight on $host ($environment, $tag)"
remote "test -f $remote_root/infrastructure/production/$env_file" ||
  { echo "Missing $remote_root/infrastructure/production/$env_file on $host" >&2; exit 1; }
previous="$(remote "docker inspect -f '{{.Config.Image}}|{{index .Config.Labels \"com.docker.compose.project.config_files\"}}' $project-api-1 2>/dev/null || true")"
previous_image="${previous%%|*}"
previous_compose="${previous#*|}"
echo "Running now: ${previous_image:-nothing}"
# Never pass /dev/null to curl itself: under MSYS_NO_PATHCONV (set by deploy-remote.ps1) Windows
# curl.exe gets the literal path, fails with exit 23 and every deploy would roll back.
http_status() { curl -s --max-time 10 -w '%{http_code}' "$1" 2>/dev/null | tail -c 3 || true; }
hallowfest_before="$(http_status "$hallowfest_health")"

if [[ "$build_on_server" == 1 ]]; then
  log "Building safeeat-server:$tag on $host (low priority)"
  # Only what the Dockerfile copies; the tar on stdin is the build context.
  git ls-files -co --exclude-standard -- Cargo.toml Cargo.lock crates apps/server db/migrations       infrastructure/production/server.Dockerfile |
    while IFS= read -r file; do if [[ -e "$file" ]]; then printf '%s
' "$file"; fi; done |
    tar -czf - -T - |
    remote "nice -n 10 docker build -f infrastructure/production/server.Dockerfile -t safeeat-server:$tag -"
else
  log "Building safeeat-server:$tag locally"
  docker build -f infrastructure/production/server.Dockerfile -t "safeeat-server:$tag" .

  log 'Streaming image to the server'
  docker save "safeeat-server:$tag" | gzip -1 | remote 'gunzip | docker load'
fi

log "Shipping deploy files to $remote_root/releases/$tag"
git ls-files -co --exclude-standard -- infrastructure/production scripts db/migrations |
  grep -Ev '(^|/)(\.env|[^/]*\.env)$' |
  while IFS= read -r file; do if [[ -e "$file" ]]; then printf '%s\n' "$file"; fi; done |
  tar -czf - -T - |
  remote "mkdir -p $remote_root/releases/$tag && tar -xzf - -C $remote_root/releases/$tag"

if [[ "$environment" == production ]]; then
  log 'Verified backup of the production database'
  remote "bash $remote_root/scripts/backup-postgres.sh"
fi

rollback() {
  log "ROLLBACK to ${previous_image:-nothing}"
  if [[ -z "$previous_image" || -z "$previous_compose" ]]; then
    echo 'No previous deployment recorded; leaving containers as they are.' >&2
    return
  fi
  # The migrations are additive, so the previous image runs on the new schema.
  remote "cd \"\$(dirname '$previous_compose')\" && IMAGE_TAG='${previous_image##*:}' \
    SAFEEAT_ENV_FILE=$remote_root/infrastructure/production/$env_file \
    docker compose -f '$previous_compose' --env-file $remote_root/infrastructure/production/$env_file \
    up -d --wait --wait-timeout 120 api web" || echo 'Rollback failed: check the server by hand.' >&2
}

log 'Deploying (migrations, api, web)'
if ! remote "cd $remote_root/releases/$tag && IMAGE_TAG='$tag' SAFEEAT_SKIP_BUILD=1 \
    bash scripts/deploy-server.sh $remote_root/infrastructure/production/$env_file"; then
  rollback
  exit 1
fi

log "Checking $public_origin$prefix through the Funnel"
ok=0
for _ in $(seq 1 10); do
  meta="$(curl -sf --max-time 10 "$public_origin$prefix/api/v1/meta" || true)"
  if [[ "$meta" == *"\"environment\":\"$environment\""* && "$meta" == *"\"version\":\"$version\""* ]] &&
    [[ "$(http_status "$public_origin$prefix/")" == 2?? ]]; then
    ok=1
    break
  fi
  sleep 3
done
if [[ "$ok" != 1 ]]; then
  echo "Public check failed (meta: ${meta:-no answer})" >&2
  rollback
  exit 1
fi
echo "$meta"

# Later compose commands (catalog import, manual runs) read IMAGE_TAG from the env file
# (`preproduction`, or `local` when unset): point that tag at the image now running.
remote "env_tag=\$(sed -n 's/^IMAGE_TAG=//p' $remote_root/infrastructure/production/$env_file | tail -1) && \
  docker tag safeeat-server:$tag safeeat-server:\${env_tag:-local}"

# Keep the five newest releases plus whatever either environment is running (its compose
# file is the rollback path of the next deploy).
remote "ln -sfn releases/$tag $remote_root/current-$environment && cd $remote_root/releases && \
  keep=\" \$(readlink ../current-preproduction 2>/dev/null || true) \$(readlink ../current-production 2>/dev/null || true) \" && \
  ls -1dt */ | tail -n +6 | while read -r dir; do dir=\${dir%/}; \
    case \"\$keep\" in *\" releases/\$dir \"*) ;; *) rm -rf -- \"\$dir\" ;; esac; done"
if [[ "$environment" == production ]]; then
  # Keep the legacy paths used by the backup/catalog systemd units in step with production.
  # The first time, keep the TypeScript-era compose file: it is the only one that can start
  # the old safeeat-api/safeeat-web images.
  remote "cd $remote_root/infrastructure/production && \
    sudo cp -n docker-compose.yml docker-compose.ts.yml && \
    sudo cp -r $remote_root/releases/$tag/scripts/. $remote_root/scripts/ && \
    sudo cp $remote_root/releases/$tag/infrastructure/production/docker-compose.yml \
      $remote_root/releases/$tag/infrastructure/production/server.Dockerfile ./"
fi

hallowfest_after="$(http_status "$hallowfest_health")"
echo "HallowFest health: before $hallowfest_before, after $hallowfest_after"
[[ "$hallowfest_before" == "$hallowfest_after" ]] || echo 'WARNING: HallowFest health changed; check it.' >&2
log "SafeEat $version ($tag) live on $environment"
