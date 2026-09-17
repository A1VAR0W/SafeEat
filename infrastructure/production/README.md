# Isolated production deployment

Use one API replica because provider budgets are local to its process. Requires Docker Compose, outbound HTTPS and an existing or dedicated TLS proxy. Inspect existing services and ports first.

1. Place the source at `/opt/safeeat`, owned by the deployment user. Preserve `.env` across deployments.
2. Copy `.env.example` in this directory to `.env`, mode `0600`. Generate unique random hex values: 32 bytes for PostgreSQL and 48 bytes for each JWT secret. Do not use URL-reserved characters in the DB password unless URL-encoded.
3. Set `API_PUBLIC_URL`, `PUBLIC_APP_URL` to real HTTPS origins. Both may be the same host. Provider keys stay here, never in the mobile build.
4. Run `bash scripts/deploy-server.sh` from the repository root. Images build before replacement, database readiness precedes transactional migrations, then API/web readiness is checked. This is not a zero-downtime/automatic-rollback deployment; image builds preserve the old service until replacement.
5. Install an isolated Nginx site from `nginx.conf.template`. Substitute only `SAFEEAT_DOMAIN` with `envsubst '${SAFEEAT_DOMAIN}'`, validate `nginx -t`, then reload. Do not overwrite other sites. Obtain the TLS certificate before enabling the HTTPS block.

Internal endpoints: `127.0.0.1:4100/health`, `127.0.0.1:4100/ready`, `127.0.0.1:3100/health`. PostgreSQL has no host-published port and uses a persistent named volume on an internal network. API/web are loopback-only; expose only Nginx 443 (and 80 for redirects/ACME if used). Public readiness exposes only a status.

DNS A/AAAA must point to the server. On a home server, the router must forward TCP 443 to it; forward TCP 80 only for HTTP ACME/redirects. A DNS record alone does not create reachability. Tailscale private access requires an authenticated daemon; Funnel is a separate explicit public publishing configuration. Do not alter unrelated Tailscale routes.

## Current Funnel ingress

The current public preproduction origin is `https://chat-ink.tail552c89.ts.net:10000` (web plus `/api/v1`). `/health` checks web, `/api-health` checks API and `/ready` checks API/database. Both mobile build variables in GitHub environment `preproduction` point here. DuckDNS is not yet a working production origin.

`nginx-funnel.conf` is installed as `/etc/nginx/conf.d/safeeat-funnel.conf`. It listens only on `127.0.0.1:5100`, requires PROXY protocol v1, and overwrites forwarded client headers using trusted local connection metadata. The matching Tailscale configuration is:

```sh
sudo nginx -t && sudo systemctl reload nginx
sudo tailscale funnel --bg --tls-terminated-tcp=10000 --proxy-protocol=1 tcp://127.0.0.1:5100
```

Do not reset Funnel: ports 443/8443 belong to Chat-Ink. Private SafeEat Serve routes remain `https://chat-ink.tail552c89.ts.net:9443` (web) and `:9444` (API). Validate certificates normally; never use an insecure TLS bypass. To disable only the SafeEat public route: `sudo tailscale funnel --tls-terminated-tcp=10000 off`.

Verified app links require SafeEat's own port-443 domain with matching association files. The current port-10000 deployment uses the custom scheme fallback. DuckDNS requires router/WAN ingress and a certificate; see `docs/VALIDATION.md` for observed blockers.

## Backup operations

`bash scripts/backup-postgres.sh` creates a timestamped custom-format dump under `/opt/safeeat/backups/postgres`, checks it with `pg_restore --list` and removes only SafeEat dumps older than 14 days. Files and directory use restricted permissions. Install the provided systemd service/timer after adjusting the user/path; start with `systemctl enable --now safeeat-backup.timer`. Run and verify one backup before claiming backup coverage.

Restore into a **new isolated database first**:

```sh
cd /opt/safeeat/infrastructure/production
docker compose exec -T postgres createdb -U safeeat safeeat_restore_check
docker compose exec -T postgres pg_restore -U safeeat -d safeeat_restore_check --no-owner --exit-on-error < /opt/safeeat/backups/postgres/safeeat-TIMESTAMP.dump
docker compose exec -T postgres psql -U safeeat -d safeeat_restore_check -c 'SELECT count(*) FROM schema_migrations'
```

Do not overwrite a live database before taking a backup and planning the maintenance window. Production account erasure reaches primary storage immediately; old backup copies expire with retention.

## Updating source over SSH

Transfer a reviewed `git archive` or fetch a pinned commit into this isolated checkout; never embed GitHub or SSH credentials into source. Run the deployment script as the existing deployment user. Only bind new SafeEat services to free loopback ports. Keep the last image tags for manual rollback and review migrations before upgrading; current migrations are additive.
