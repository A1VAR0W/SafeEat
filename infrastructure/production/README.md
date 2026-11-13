# Isolated production deployment

## Fixed rules (the server is shared with HallowFest)

- Never run `tailscale funnel reset` or change Funnel/Serve routes other than SafeEat's `:10000`.
- Never run `docker compose down -v` (or remove a SafeEat volume) on either deployment.
- Never bind to or touch ports 443, 3307, 3308 or 3399, nginx sites other than `safeeat-funnel.conf`, or HallowFest's containers.
- Production deploys start with a verified backup (`scripts/deploy-remote.sh` does it); migrations stay additive while a TypeScript image may still be rolled back to.
- SafeEat uses no paid APIs (2.0.5): no Anthropic or OpenAI key. Label photos are read on the phone; the only model is the self-hosted llama.cpp (`AI_LOCAL_URL`). A leftover `ANTHROPIC_API_KEY` in an env file is ignored.

## Deploying from a workstation (SafeEat 2.0, Rust)

`scripts/deploy-remote.sh` (or `scripts\deploy-remote.ps1` on Windows) deploys over `ssh chatink-dev`; the default target is preproduction:

```sh
bash scripts/deploy-remote.sh                      # preproduction
bash scripts/deploy-remote.sh --env production     # asks for confirmation, backs up first
```

It builds `safeeat-server:<git sha>` locally (one image for the API, the web viewer, migrations and the OSM import), streams it with `docker save | ssh docker load`, ships the deploy files to `/opt/safeeat/releases/<tag>` (`current-<env>` points at the live one; env files stay in `/opt/safeeat/infrastructure/production`), runs `scripts/deploy-server.sh` with `IMAGE_TAG=<tag> SAFEEAT_SKIP_BUILD=1`, then checks `/api/v1/meta` and the web root through the Funnel. If anything fails it restarts the previous image with its own compose file (recorded from the running container), which works because migrations are additive. After a production deploy the first TypeScript-era compose file is kept as `docker-compose.ts.yml`.

The containers are bounded (`api` 384 MB / 1.5 CPU, `web` 128 MB / 0.5 CPU, read-only root filesystems); the Rust API idles at a few MB.

Use one API replica because provider budgets are local to its process. Requires Docker Compose, outbound HTTPS and an existing or dedicated TLS proxy. Inspect existing services and ports first.

1. Place the source at `/opt/safeeat`, owned by the deployment user. Preserve `.env` across deployments.
2. Copy `.env.example` in this directory to `.env`, mode `0600`. Generate unique random hex values: 32 bytes for PostgreSQL and 48 bytes for each JWT secret. Do not use URL-reserved characters in the DB password unless URL-encoded.
3. Set `APP_ENV=production`, `COMPOSE_PROJECT_NAME=safeeat`, and `API_PUBLIC_URL` / `PUBLIC_APP_URL` to the verified HTTPS base URL. Both may be the same host. Provider keys stay here, never in the mobile build. Production startup rejects placeholder secrets and development environment settings.
4. Run `bash scripts/deploy-server.sh` from the repository root (or use `scripts/deploy-remote.sh` above). The image builds before replacement, database readiness precedes transactional migrations (`safeeat-server migrate`), then API/web readiness is checked with `safeeat-server healthcheck`.
5. Install an isolated Nginx site from `nginx.conf.template`. Substitute only `SAFEEAT_DOMAIN` with `envsubst '${SAFEEAT_DOMAIN}'`, validate `nginx -t`, then reload. Do not overwrite other sites. Obtain the TLS certificate before enabling the HTTPS block.

Internal endpoints: `127.0.0.1:4100/health`, `127.0.0.1:4100/ready`, `127.0.0.1:3100/health`. PostgreSQL has no host-published port and uses a persistent named volume on an internal network. API/web are loopback-only; expose only Nginx 443 (and 80 for redirects/ACME if used). Public readiness exposes only a status.

DNS A/AAAA must point to the server. On a home server, the router must forward TCP 443 to it; forward TCP 80 only for HTTP ACME/redirects. A DNS record alone does not create reachability. Tailscale private access requires an authenticated daemon; Funnel is a separate explicit public publishing configuration. Do not alter unrelated Tailscale routes.

## Current Funnel ingress

Status: **PROVISIONAL PRODUCTION / PUBLIC TESTING**. Production is `https://chat-ink.tail552c89.ts.net:10000` (web plus `/api/v1`). Preproduction is `https://chat-ink.tail552c89.ts.net:10000/preproduction`. Append `/health` for web, `/api-health` for API, `/ready` for database readiness, and `/api/v1/meta` for safe environment/version metadata. The GitHub Environments `production` and `preproduction` each contain their own matching API/web base URL. Public Funnel access needs no Tailscale installation on the phone. DuckDNS is not yet a working production origin.

Production uses `.env`, project `safeeat`, API/web loopback ports 4100/3100 and volume `safeeat_postgres-data`. Preproduction uses `preproduction.env` (0600), project `safeeat-preproduction`, ports 4200/3200, volume `safeeat-preproduction_postgres-data`, independent database/JWT credentials, `IMAGE_TAG=preproduction` and `WEB_BASE_PATH=/preproduction`. The same database name `safeeat` exists inside two isolated PostgreSQL services; automated integration tests use a third disposable service and never either deployed database.

The restaurant catalog uses PostGIS in the existing PostgreSQL 17 data volume. Take and verify a backup before replacing the PostgreSQL image with `postgis/postgis:17-3.5-alpine`, then run the normal migration. Install `osmium-tool` on the deployment host and run `bash scripts/update-restaurant-catalog.sh madrid /opt/safeeat/infrastructure/production/preproduction.env` first (from `/opt/safeeat/current-preproduction`; it runs `safeeat-server import-restaurants` in the api image). Each import recomputes chains from the raw OSM `brand` / `brand:wikidata` tags (`refresh_restaurant_chains()`). Validate the three restaurant filters on preproduction, then import `spain` and `canarias` (Geofabrik publishes the Canary Islands under Africa) with the production `.env`. Each import also downloads the extract's `.poly` boundary: since migration 018 a map cell counts as covered when it lies inside an imported boundary (clipped to it), never just because a venue is nearby. The importer downloads a Geofabrik extract, keeps food venues, and atomically replaces one region; the API keeps its existing provider fallback until a catalog is loaded. The full Spain extract needs several GB of temporary disk and may take time to process. After the initial production import, install and enable `safeeat-restaurants.service` and `.timer` for weekly refreshes. OpenStreetMap attribution remains visible on restaurant details.

Deploy preproduction with `bash scripts/deploy-server.sh /opt/safeeat/infrastructure/production/preproduction.env`. Generate its secrets once and preserve them. `nginx-funnel.conf` routes the prefix to the separate API/web; the Rust web viewer reads `WEB_BASE_PATH` at start-up, so no rebuild is needed when it changes. Never run `docker compose down -v` on either deployment.

`nginx-funnel.conf` is installed as `/etc/nginx/conf.d/safeeat-funnel.conf`. It listens only on `127.0.0.1:5100`, requires PROXY protocol v1, and overwrites forwarded client headers using trusted local connection metadata. The matching Tailscale configuration is:

The `/safeeat-downloads/` location serves SideStore and LiveContainer source JSON, the icon and unsigned IPA files from `/var/www/safeeat-downloads/`. Publish the reviewed source files from `docs/sidestore-*.json` as `production.json` and `preproduction.json`, copy `production.json` to `safeeat.json`, and upload the matching IPA and `safeeat-mark.png`. Validate Nginx and the public JSON, PNG and IPA responses after each update. The JSON uses classic AltStore fields for both clients, and Nginx sends the correct media types with `Cache-Control: no-store`.

For Obtainium, publish the signed Android APK under the same directory and install `docs/obtainium-android.html` as `index.html`. Its versioned APK link lets Obtainium detect a new release and offer the update without GitHub credentials. Keep one APK link on the page and retain the same Android signing key across updates.

```sh
sudo nginx -t && sudo systemctl reload nginx
sudo tailscale funnel --bg --tls-terminated-tcp=10000 --proxy-protocol=1 tcp://127.0.0.1:5100
```

Do not reset Funnel: 443 (→ 3307) and 8443 belong to HallowFest and other services. Private SafeEat Serve routes remain `https://chat-ink.tail552c89.ts.net:9443` (production web) and `:9444` (production API); those are alternate private access to production, not the isolated preproduction environment. Validate certificates normally; never use an insecure TLS bypass. To disable only the SafeEat public route: `sudo tailscale funnel --tls-terminated-tcp=10000 off`. Docker, Nginx and tailscaled are enabled at boot, and all SafeEat containers use `unless-stopped`. A server reboot was not performed because it would interrupt unrelated services.

Verified app links require SafeEat's own port-443 domain with matching association files. The current port-10000 deployment uses the custom scheme fallback. DuckDNS requires router/WAN ingress and a certificate; see `docs/VALIDATION.md` for observed blockers.

## Backup operations

`bash scripts/backup-postgres.sh` creates a timestamped custom-format dump under `/opt/safeeat/backups/postgres`, checks it with `pg_restore --list` and removes only SafeEat dumps older than 14 days. Files and directory use restricted permissions. Install the provided systemd service/timer after adjusting the user/path; start with `systemctl enable --now safeeat-backup.timer`. Run and verify one backup before claiming backup coverage.

Restore into a **new isolated database first**:

```sh
cd /opt/safeeat/infrastructure/production
docker compose exec -T postgres createdb -U safeeat safeeat_restore_check
docker compose exec -T postgres pg_restore -U safeeat -d safeeat_restore_check --no-owner --exit-on-error < /opt/safeeat/backups/postgres/safeeat-TIMESTAMP.dump
docker compose exec -T postgres psql -U safeeat -d safeeat_restore_check -c 'SELECT count(*) FROM schema_migrations'
# After inspecting the restored schema and data, remove only this validation database:
docker compose exec -T postgres dropdb -U safeeat safeeat_restore_check
```

Do not overwrite a live database before taking a backup and planning the maintenance window. Production account erasure reaches primary storage immediately; old backup copies expire with retention.

For an actual recovery, stop only the production API/web during the maintenance window, take a fresh dump, restore the selected backup into a new named database, verify it, update the production database connection to that restored database, and restart API/web after checking migration compatibility. Retain the former database until recovery is accepted. Current backups are on the same server; off-host backup replication remains an operational improvement.

## Updating source over SSH

Use `scripts/deploy-remote.sh`; never embed GitHub or SSH credentials into source. It runs as the existing deployment user (`sudo` only to refresh the root-owned legacy paths after a production deploy). Only bind new SafeEat services to free loopback ports. The last five releases and their image tags remain for manual rollback; review migrations before upgrading.
