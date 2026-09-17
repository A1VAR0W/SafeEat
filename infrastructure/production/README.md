# Isolated production deployment

Use one API replica because provider budgets are local to its process. Requires Docker Compose, outbound HTTPS and an existing or dedicated TLS proxy. Inspect existing services and ports first.

1. Place the source at `/opt/safeeat`, owned by the deployment user. Preserve `.env` across deployments.
2. Copy `.env.example` in this directory to `.env`, mode `0600`. Generate unique random hex values: 32 bytes for PostgreSQL and 48 bytes for each JWT secret. Do not use URL-reserved characters in the DB password unless URL-encoded.
3. Set `APP_ENV=production`, `COMPOSE_PROJECT_NAME=safeeat`, and `API_PUBLIC_URL` / `PUBLIC_APP_URL` to the verified HTTPS base URL. Both may be the same host. Provider keys stay here, never in the mobile build. Production startup rejects placeholder secrets and development environment settings.
4. Run `bash scripts/deploy-server.sh` from the repository root. Images build before replacement, database readiness precedes transactional migrations, then API/web readiness is checked. This is not a zero-downtime/automatic-rollback deployment; image builds preserve the old service until replacement.
5. Install an isolated Nginx site from `nginx.conf.template`. Substitute only `SAFEEAT_DOMAIN` with `envsubst '${SAFEEAT_DOMAIN}'`, validate `nginx -t`, then reload. Do not overwrite other sites. Obtain the TLS certificate before enabling the HTTPS block.

Internal endpoints: `127.0.0.1:4100/health`, `127.0.0.1:4100/ready`, `127.0.0.1:3100/health`. PostgreSQL has no host-published port and uses a persistent named volume on an internal network. API/web are loopback-only; expose only Nginx 443 (and 80 for redirects/ACME if used). Public readiness exposes only a status.

DNS A/AAAA must point to the server. On a home server, the router must forward TCP 443 to it; forward TCP 80 only for HTTP ACME/redirects. A DNS record alone does not create reachability. Tailscale private access requires an authenticated daemon; Funnel is a separate explicit public publishing configuration. Do not alter unrelated Tailscale routes.

## Current Funnel ingress

Status: **PROVISIONAL PRODUCTION / PUBLIC TESTING**. Production is `https://chat-ink.tail552c89.ts.net:10000` (web plus `/api/v1`). Preproduction is `https://chat-ink.tail552c89.ts.net:10000/preproduction`. Append `/health` for web, `/api-health` for API, `/ready` for database readiness, and `/api/v1/meta` for safe environment/version metadata. The GitHub Environments `production` and `preproduction` each contain their own matching API/web base URL. Public Funnel access needs no Tailscale installation on the phone. DuckDNS is not yet a working production origin.

Production uses `.env`, project `safeeat`, API/web loopback ports 4100/3100 and volume `safeeat_postgres-data`. Preproduction uses `preproduction.env` (0600), project `safeeat-preproduction`, ports 4200/3200, volume `safeeat-preproduction_postgres-data`, independent database/JWT credentials, `IMAGE_TAG=preproduction` and `WEB_BASE_PATH=/preproduction`. The same database name `safeeat` exists inside two isolated PostgreSQL services; automated integration tests use a third disposable service and never either deployed database.

Deploy preproduction with `bash scripts/deploy-server.sh /opt/safeeat/infrastructure/production/preproduction.env`. Generate its secrets once and preserve them. `nginx-funnel.conf` routes the prefix to the separate API/web; the web base path is also compiled into its Next.js image. Rebuild that image if the base path changes. Never run `docker compose down -v` on either deployment.

`nginx-funnel.conf` is installed as `/etc/nginx/conf.d/safeeat-funnel.conf`. It listens only on `127.0.0.1:5100`, requires PROXY protocol v1, and overwrites forwarded client headers using trusted local connection metadata. The matching Tailscale configuration is:

```sh
sudo nginx -t && sudo systemctl reload nginx
sudo tailscale funnel --bg --tls-terminated-tcp=10000 --proxy-protocol=1 tcp://127.0.0.1:5100
```

Do not reset Funnel: ports 443/8443 belong to Chat-Ink. Private SafeEat Serve routes remain `https://chat-ink.tail552c89.ts.net:9443` (production web) and `:9444` (production API); those are alternate private access to production, not the isolated preproduction environment. Validate certificates normally; never use an insecure TLS bypass. To disable only the SafeEat public route: `sudo tailscale funnel --tls-terminated-tcp=10000 off`. Docker, Nginx and tailscaled are enabled at boot, and all SafeEat containers use `unless-stopped`. A server reboot was not performed because it would interrupt unrelated services.

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

Transfer a reviewed `git archive` or fetch a pinned commit into this isolated checkout; never embed GitHub or SSH credentials into source. Run the deployment script as the existing deployment user. Only bind new SafeEat services to free loopback ports. Keep the last image tags for manual rollback and review migrations before upgrading; current migrations are additive.
