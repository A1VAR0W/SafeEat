# Validation — 2026-09-17 (Europe/Madrid)

PASS means executed successfully. FAIL means an observed unresolved failure. NOT RUN means not executed/completed. BLOCKED EXTERNALLY means an identified missing external dependency. These checks are not a clinical certification or exhaustive security testing.

## Automated checks

| Check                         | Status  | Evidence                                                                                                                                                                                                                                          |
| ----------------------------- | ------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `npm ci`                      | PASS    | Node 22.22.3; lockfile installation locally and CI                                                                                                                                                                                                |
| Lint / formatting / typecheck | PASS    | Executed locally and GitHub CI                                                                                                                                                                                                                    |
| Unit tests                    | PASS    | 93: gluten 62, nutrition 9, contracts 2, OFF 3, env 1, mobile session/cache 7, release origins 9                                                                                                                                                  |
| PostgreSQL integration        | PASS    | 20 tests against separate PostgreSQL 17.9 container over SSH; never the deployed DB                                                                                                                                                               |
| API / Next production build   | PASS    | `npm run build`; public routes generated                                                                                                                                                                                                          |
| Expo dependencies             | PASS    | `npm run mobile:check`                                                                                                                                                                                                                            |
| Expo Doctor                   | PASS    | 20/20 checks                                                                                                                                                                                                                                      |
| Android / iOS JS exports      | PASS    | Local dual export and CI [35159039342](https://github.com/A1VAR0W/SafeEat/actions/runs/35159039342), including final mobile code at `54a5102`                                                                                                     |
| Latest backend CI             | NOT RUN | [35159280312](https://github.com/A1VAR0W/SafeEat/actions/runs/35159280312) executing at documentation update                                                                                                                                      |
| Android APK/AAB               | NOT RUN | Final mobile build [35159039498](https://github.com/A1VAR0W/SafeEat/actions/runs/35159039498) executing; initial SDK failure fixed                                                                                                                |
| Unsigned iOS IPA              | PASS    | [35158551691](https://github.com/A1VAR0W/SafeEat/actions/runs/35158551691), Info.plist/Payload/ZIP checked; downloaded SHA-256 matched. Final mobile rebuild [35159042427](https://github.com/A1VAR0W/SafeEat/actions/runs/35159042427) executing |
| Installed Android smoke       | NOT RUN | Workflow prepared to download, verify, install and launch actual APK; awaiting final Android build                                                                                                                                                |

## Deployed service evidence

Canonical **preproduction** origin: `https://chat-ink.tail552c89.ts.net:10000`. GitHub `preproduction` has this value in both `API_PUBLIC_URL` and `PUBLIC_APP_URL`. DuckDNS has not been promoted to a production build target.

| Check                       | Status | Evidence                                                                                                                                        |
| --------------------------- | ------ | ----------------------------------------------------------------------------------------------------------------------------------------------- |
| Public TLS / web health     | PASS   | Local machine, public Funnel DNS; `/health` 200, certificate validation enabled                                                                 |
| API health / readiness      | PASS   | `/api-health` and `/ready` 200 through public ingress                                                                                           |
| Private Tailscale endpoints | PASS   | Server-host HTTPS requests to `:9443/health`, `:9444/ready`, certificate validation enabled; ports require tailnet membership                   |
| Containers                  | PASS   | API, web, PostgreSQL healthy; `unless-stopped`; unrelated services preserved                                                                    |
| PostgreSQL isolation        | PASS   | No published host port; internal `safeeat_private` network; persistent `safeeat_postgres-data` volume                                           |
| Migrations                  | PASS   | Transactional `001_initial.sql` applied; repeat deployment succeeds                                                                             |
| Permissions                 | PASS   | `.env` 0600, dump 0600, backup directory 0700                                                                                                   |
| Backup and restore          | PASS   | `safeeat-20260916T223503Z.dump`, 13,319 bytes; `pg_restore --exit-on-error` into isolated DB; one schema migration verified; restore DB removed |
| Backup scheduling           | PASS   | Timer active, next 03:40 CEST; daily script prunes old dumps; first scheduled run not yet observed                                              |
| Disk                        | PASS   | 828 GiB available during inspection                                                                                                             |

Ingress uses Tailscale TLS termination on 10000 and PROXY v1 to Nginx `127.0.0.1:5100`; Nginx trusts only local PROXY metadata and replaces X-Forwarded-For. API trusts one proxy hop. Chat-Ink public 443/8443 and SafeEat private 9443/9444 routes remain configured.

## Product and security audit

- PASS: real HTTPS register/login/logout/revocation/refresh, reviewed-label analysis, favorite add/read/remove, private-label anonymous 404, anonymous share, OG, custom-scheme link, account deletion/share revocation. Reproduce with `node scripts/smoke-deployment.mjs https://chat-ink.tail552c89.ts.net:10000`. Creates/removes its own disposable account; does not output credentials.
- PASS: live OFF barcode `3017620422003`, evidence and separate nutrition score. Legacy search returned upstream 503; official replacement endpoint returned 200. Final deployed search smoke pending.
- PASS: missing product/restaurant viewer tokens return 404; malformed API token is 400. Both association endpoints return JSON without redirects.
- PASS: integration tests cover cross-user IDOR, favorites isolation/pagination, share ownership, opaque 192-bit tokens, password/refresh hashing, immediate logout/replay revocation, concurrent rotation/logout, expired/tampered JWTs, SQL-shaped IDs, payload 413, CORS non-reflection and repeated-login 429.
- PASS: stale fallback is labelled; uncached outage is 503 rather than 404; search summaries cannot overwrite detail. Server keys are absent from Expo config. API logs serialize no request bodies, JWTs or refresh credentials; error logs use error type only.
- PASS: all requested adverse gluten phrases plus recipe/historical/unverified claims covered. Missing hazards alone never mean GLUTEN_FREE. Positive ingredients/traces override claims; unreviewed OCR never establishes safety. This is a conservative heuristic, not arbitrary natural-language understanding.
- PASS: mobile regression tests cover concurrent refresh, stale responses after logout/account changes, private-cache clearing, public offline retention, bodyless requests and unsafe release origins.
- PASS with findings: production dependency audit reports **0 high/critical, 12 moderate** transitive entries (decode-uri-component/query-string/Expo Router; uuid/Expo build tooling). Forced npm fixes suggest incompatible Expo/Router downgrades and were not applied. Compatible upstream patches remain necessary; the dependency tree is not vulnerability-free.

## UX and external limits

- PASS: deployed home and real product viewer inspected in browser at desktop width and 375×812, dark theme; readable layout without visible clipping.
- PASS (code audit): safe areas, keyboard avoidance, dark mode, reduced-motion transitions, 52dp buttons, virtualized lists, loading/error/empty states, submitted search, barcode deduplication, capture-only OCR and camera release on blur/background.
- BLOCKED EXTERNALLY: physical camera/barcode/OCR quality, permission prompts, actual iPhone notch/keyboard and signed iOS installation; no physical device or Apple signing session attached. Unsigned IPA needs re-signing.
- BLOCKED EXTERNALLY: live restaurants need `GOOGLE_PLACES_API_KEY`. Deployment returns clean `RESTAURANT_PROVIDER_NOT_CONFIGURED` 503; restaurant favorites/sharing pass integration using an explicitly injected fixture provider.
- BLOCKED EXTERNALLY: verified Universal/App Links. Apple team and Android fingerprints unset; Funnel 10000 cannot use Chat-Ink's port-443 association files. Custom `safeeat://p/<token>` and `/r/<token>` links are supplied. A SafeEat canonical 443 domain and matching signing configuration are needed.
- BLOCKED EXTERNALLY: DuckDNS A=`81.0.45.99`, matching server outbound IP. HTTP80/HTTPS443 time out from this machine. Nginx listens on LAN80; no SafeEat Let's Encrypt certificate exists; only Tailscale owns tailnet443. Router forwarding/WAN ingress cannot be diagnosed or changed with available SSH alone. Prior DNS-01 attempt found the existing DuckDNS token did not authorize this domain. Correct domain credentials or working HTTP-01 ingress are needed; preserve Chat-Ink routes.

## Artifacts

Workflow URLs above. GitHub artifacts expire after 14 days; downloads are under ignored `artifacts/`. Expected files: `SafeEat-0.1.0.apk`, `.aab`, `.ipa`, each with `.sha256`. Android signing/badging and emulator evidence are separate. Preproduction APK uses a testing key; IPA is intentionally unsigned. Neither is a store publication.
