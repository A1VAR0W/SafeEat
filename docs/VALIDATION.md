# Validation — 2026-09-18 (Europe/Madrid)

## Final V2 release candidate — 2026-09-18

PASS: commit `856a608` is deployed to production and preproduction. Public production restaurant search with the gluten-free filter returned 35 nearby venues in Madrid; preproduction exercised the explicit-name fallback when the OpenStreetMap query provider was saturated. The fallback is labelled as a lead to verify with the venue, never as a gluten-safety certification.

PASS: GitHub CI runs `35357237776` and `35357239462` completed successfully for `856a608`. They cover formatting, lint, TypeScript, 110 unit tests, PostgreSQL integration tests, production builds, Expo dependency/Doctor checks, native JavaScript exports, infrastructure validation and workflow validation. Android and iOS native release builders succeeded for the unchanged mobile source at `9781b6d`; later commits change only the server-side restaurant fallback.

The release architecture uses Apple Maps on iOS and MapLibre/OpenFreeMap on Android, with OpenStreetMap/Photon/Overpass as the restaurant data provider. No Google API key is configured or required. Public provider capacity is appropriate for private use; the service returns a controlled fallback or error if an upstream provider is unavailable.

## V2 UI milestone (not a release validation)

PASS: 99 unit tests, mobile typecheck, lint/format and Android/iOS exports. A local Android API 35 UI preview used the existing preproduction native shell with the new exported Hermes bundle, separately re-signed under ignored .local; the six stable 0.1.0 artifacts were unchanged. Explore, recent products, compact detail, profile menu and four native sheets were exercised with real preproduction product data. Captures at 411×914 and 360×640 dp, plus 1.3× system text, are under .local/v2-ui. Detail controls adapt to one column for large text.

The local emulator needed a restricted host CONNECT proxy for Internet access; this preview does not replace the final V2 standalone Release test. Physical torch/camera quality, iOS installation, full new account flows and final V2 deployment remain NOT RUN.

PASS means executed successfully. FAIL means an observed unresolved failure. NOT RUN means not executed/completed. BLOCKED EXTERNALLY means an identified missing external dependency. Software checks are not a clinical certification or exhaustive security testing.

## Environments

Status: **PROVISIONAL PRODUCTION / PUBLIC TESTING**. Both remote environments are publicly reachable without a Tailnet account, Metro, development computer or SSH tunnel.

| Environment   | Web / API base                                           | Database                                                                                     |
| ------------- | -------------------------------------------------------- | -------------------------------------------------------------------------------------------- |
| Development   | `http://localhost:3000` / `http://localhost:4000`        | Local development Compose                                                                    |
| Production    | `https://chat-ink.tail552c89.ts.net:10000`               | Private `safeeat-postgres-1`, persistent `safeeat_postgres-data`                             |
| Preproduction | `https://chat-ink.tail552c89.ts.net:10000/preproduction` | Private `safeeat-preproduction-postgres-1`, persistent `safeeat-preproduction_postgres-data` |

API routes append `/api/v1`. `/api/v1/meta` returns the intended environment and version `0.1.0`, without internal addresses or secrets. GitHub Environments contain matching public build variables. Independent database/JWT secrets were verified without disclosing values; both server configuration files have mode 0600. The same disposable email registered separate user IDs in both environments; cross-environment access and refresh tokens were rejected with 401. Both accounts were removed.

Ingress uses Tailscale TLS termination on 10000 and PROXY v1 to Nginx `127.0.0.1:5100`. Nginx trusts only local PROXY metadata and replaces X-Forwarded-For; API trusts one proxy hop. Production API/web use loopback 4100/3100; preproduction uses 4200/3200. PostgreSQL has no published host port. Chat-Ink public 443/8443 are preserved; private SafeEat 9443/9444 remain alternate access to production.

## Automated checks

| Check                            | Status | Evidence                                                                                                                                          |
| -------------------------------- | ------ | ------------------------------------------------------------------------------------------------------------------------------------------------- |
| `npm ci`                         | PASS   | Node 22.22.3; lockfile installation locally and GitHub CI                                                                                         |
| Format / lint / typecheck        | PASS   | Local checks; complete CI on `1dcc3ea`                                                                                                            |
| Unit tests                       | PASS   | 97: gluten 62, nutrition 9, contracts 3, OFF 3, env 1, mobile session/cache 9, release config 10                                                  |
| PostgreSQL integration           | PASS   | 20 tests, separate PostgreSQL 17.9 container/database; temporary container and tunnel removed                                                     |
| API / Next production build      | PASS   | CI [35168007239](https://github.com/A1VAR0W/SafeEat/actions/runs/35168007239); both deployed images built and started                             |
| Expo dependency check / Doctor   | PASS   | CI above, Doctor 20/20                                                                                                                            |
| Android / iOS JS exports         | PASS   | CI above, production HTTPS configuration                                                                                                          |
| Latest native-code CI            | PASS   | [35169035225](https://github.com/A1VAR0W/SafeEat/actions/runs/35169035225), native source `9dfd4fb`                                               |
| Production APK/AAB               | PASS   | [35169037890](https://github.com/A1VAR0W/SafeEat/actions/runs/35169037890); both files downloaded, SHA-256 matched                                |
| Preproduction APK/AAB            | PASS   | [35169040419](https://github.com/A1VAR0W/SafeEat/actions/runs/35169040419); both files downloaded, SHA-256 matched                                |
| Production unsigned IPA          | PASS   | [35169043171](https://github.com/A1VAR0W/SafeEat/actions/runs/35169043171); downloaded SHA-256 matched                                            |
| Preproduction unsigned IPA       | PASS   | [35169045379](https://github.com/A1VAR0W/SafeEat/actions/runs/35169045379); downloaded SHA-256 matched                                            |
| Installed production Release APK | PASS   | [35194870325](https://github.com/A1VAR0W/SafeEat/actions/runs/35194870325): all nine installed Internet checks passed; disposable account removed |

## Native connectivity diagnosis

The older installed APK started normally, but its missing-share test never reached the expected server 404: [failed installed run 35167309494](https://github.com/A1VAR0W/SafeEat/actions/runs/35167309494). The app displayed a connection error.

The request wrapper used `AbortSignal.timeout(15000)`. React Native 0.83 installs `abort-controller/dist/abort-controller`, whose `AbortSignal.timeout` is undefined. Every request failed before calling fetch. This was replaced with `new AbortController()` and a timeout cleared in `finally`; a regression test removes `AbortSignal.timeout` and verifies fetch is invoked. Tests also cover timeout classification and bodyless POST/DELETE headers.

Release builds require explicit environment variables, reject local/IP/non-HTTPS URLs and check the actual public API environment before prebuild. Final archives are inspected for embedded API/web URLs and standalone JavaScript bundles. Android checks include INTERNET permission, non-debuggable Release, package/version/build and signature. iOS checks include bundle/version/build, Payload, Info.plist and ZIP integrity. Settings shows the actual server environment/version. DNS and TLS failures share a controlled secure-connection error because React Native fetch does not reliably distinguish those causes; offline, timeout, HTTP and auth errors remain separate.

The installed test downloads the actual artifact, checks SHA-256, installs it in an Android API 35 emulator on GitHub, verifies no `adb reverse`, and runs without Node/Metro or SSH. Assertions cover registration, login, logout, process-restart session restoration, OFF search/full detail, favorites persisted in the public API's database, native sharing/anonymous OG viewer, unfavorite and final logout. Disposable accounts are removed through the public API. Only an executed successful run counts as PASS.

## Deployed service and backup evidence

| Check                                     | Status  | Evidence                                                                                                                                                      |
| ----------------------------------------- | ------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Public production and preproduction HTTPS | PASS    | Certificate validation enabled; `/health`, `/api-health`, `/ready`, metadata all 200                                                                          |
| Containers / migrations                   | PASS    | Both API/web/PostgreSQL stacks healthy; transactional initial migration applied; repeat deployment successful                                                 |
| Isolation / restart configuration         | PASS    | Separate projects, networks, volumes and credentials; `unless-stopped`; Docker/Nginx/tailscaled enabled at boot                                               |
| Actual reboot recovery                    | NOT RUN | Reboot would interrupt unrelated services; persistent background Funnel inspected                                                                             |
| Production backup and restore             | PASS    | `safeeat-20260917T005206Z.dump`, 16,036 bytes, mode 0600; pg_restore into separate temporary PostgreSQL container; migration count 1; restored DB removed     |
| Backup schedule / retention               | PASS    | Active daily 03:40 CEST timer; 14-day pruning; directory 0700. Scheduled run at 03:40 CEST succeeded (ExecMainStatus=0), dump `safeeat-20260917T014001Z.dump` |
| Disk capacity                             | PASS    | 822 GiB available                                                                                                                                             |

Backups currently stay on the same server; off-host copies remain an operational improvement. Dump and isolated-restore commands are in [the deployment guide](../infrastructure/production/README.md).

## Public end-to-end, safety and security

- PASS in **both** environments: `node scripts/smoke-deployment.mjs <base-url> <environment>`. HTTPS register/login/logout/revocation/refresh/me, reviewed-label analysis, bodyless favorite add/remove, favorites list/membership, anonymous private-label denial, public share/OG/custom scheme, account deletion and share revocation. Environment reports are saved under ignored `.local/`; no credentials are printed.
- PASS: real OFF barcode `3017620422003`, evidence and separate nutrition. Official Search-a-licious query `arroz` returns only UNKNOWN summaries; selecting the first result retrieves canonical details. Search summaries cannot overwrite full cached evidence. Legacy search returned 503 and was replaced.
- PASS: anonymous product viewer, product/restaurant missing-token 404, malformed API token 400, association JSON without redirects. Browser inspection covered desktop and 375×812 dark layout without visible clipping.
- PASS: 20 integration tests cover cross-user IDOR, favorites isolation/pagination, share ownership, opaque 192-bit tokens, password/refresh hashing, logout/replay revocation, concurrent rotation/logout, expired/tampered JWTs, SQL-shaped IDs, oversized input 413, CORS non-reflection and repeated-login 429.
- PASS: stale provider fallback is labelled; uncached outage is 503 rather than 404. Server secrets are absent from Expo config. API logs serialize no bodies or credentials; errors log only their type.
- PASS: 62 gluten regressions include positive/negated claims, contradictory ingredients/traces, oats, Unicode, recipe/historical/unverified claims and reviewed/unreviewed OCR. Missing hazards never establish GLUTEN_FREE. This remains a conservative heuristic, not arbitrary natural-language understanding.
- PASS: mobile session regressions cover single refresh, stale responses after logout/account changes and private-cache clearing with public offline retention. Nutrition is independently tested and returns no score for incomplete input.
- PASS with findings: production dependency audit reported **0 high/critical, 12 moderate transitive entries** (decode-uri-component/query-string/Expo Router; uuid/Expo tooling). Forced npm fixes proposed incompatible Expo/Router downgrades and were not applied. Compatible upstream updates remain necessary; this is not a claim of zero vulnerabilities.

## External limits and remaining device checks

- BLOCKED EXTERNALLY: live restaurant data requires `GOOGLE_PLACES_API_KEY`. Both environments return controlled `RESTAURANT_PROVIDER_NOT_CONFIGURED` 503. Restaurant favorites/sharing pass isolated integration tests with an injected fixture provider.
- BLOCKED EXTERNALLY: DuckDNS A=`81.0.45.99`, matching server outbound IPv4. TCP80/443 time out from this machine; no SafeEat Let's Encrypt certificate exists. Router forwarding/WAN ingress and possible ISP/CGNAT restrictions cannot be determined or changed with server SSH alone. Prior DNS-01 attempt found the existing DuckDNS token did not authorize this domain. Correct credentials or reachable HTTP-01 ingress are needed. Public Funnel keeps testing usable.
- BLOCKED EXTERNALLY: verified Universal/App Links need a SafeEat port-443 domain, Apple Team ID and matching Android signing fingerprints. Port443 association files belong to Chat-Ink and were preserved. Environment-aware Android path filters and Expo native link rewriting are prepared; `safeeat://p/<token>` / `r/<token>` fallback remains available.
- NOT RUN: physical Android on 4G/5G, physical camera/barcode/OCR quality and permission interactions; no device attached. The GitHub emulator test is a distinct public-Internet proof.
- BLOCKED EXTERNALLY: iPhone/SideStore installation and real iOS camera/notch/keyboard validation require an attached device and re-signing session. IPA generation does not prove installed iOS behavior.
- Current Android APKs use a testing key, not final Play signing. Production signing is configurable through GitHub secrets. Both channels retain `com.a1var0w.safeeat` and replace each other on one device.

## Artifacts

Builders now publish only the installable APK or IPA for each target. Signature, package, embedded configuration and archive integrity are checked during the workflow; auxiliary validation files are not published. Downloads and emulator screenshots/XML/logs stay in ignored `artifacts/`. GitHub artifacts expire after 14 days. The table below records a historical validation run.

| Artifact          | SHA-256                                                          |
| ----------------- | ---------------------------------------------------------------- |
| Production APK    | 3eff58750ff7784ffa42ad3576e518f454417b3559cd745043bf38a0788d5fdf |
| Production AAB    | 474ccc3b6e2dc362f5f2c9c05feb5c2f3c7233a125a1f56c21b255dbeb5762e5 |
| Production IPA    | f75781abd5fb46ae4b64c23c4ee278a412819f189547fa798097083c5f898d3f |
| Preproduction APK | 4991322ed4afd4529852c6b70a198e83fa9a91e746f95f0c0d921f54fdf8f45a |
| Preproduction AAB | 26ce50696d4f3060e379ba4da09c14764f38ab3297d23cee2df8b404d29a8e71 |
| Preproduction IPA | ed49d22827e55b26b49304155a535f6165f3f46d2560305a0e317b6f3aadeac0 |
