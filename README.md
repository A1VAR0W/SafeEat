# SafeEat

React Native app for evidence-based gluten information, ingredient-label OCR, nutrition, favorites, restaurants and public sharing. Spanish first. Gluten-free is **not** synonymous with healthy, and absence of detected gluten is **not** evidence of safety.

## Run locally

Node **22.22.3**, npm, Docker Compose, and an Android SDK/JDK 17 or macOS/Xcode are required. This is a development-build app, not Expo-Go-only.

```sh
npm ci
cp .env.example .env
docker compose up -d --wait
npm run db:migrate
npm run dev
```

API: `http://localhost:4000`; public viewer: `http://localhost:3000`. In another terminal:

```sh
# Set API_PUBLIC_URL to your computer's LAN address for a physical device.
# PowerShell: $env:API_PUBLIC_URL='http://192.168.x.x:4000'
# Bash: export API_PUBLIC_URL=http://192.168.x.x:4000
npm run android -w @safeeat/mobile
# On macOS:
npm run ios -w @safeeat/mobile
# After installing the development client:
npm run mobile
```

PowerShell users can use `Copy-Item .env.example .env` instead of `cp`. Set variables in the terminal launching Expo; no private API key belongs in Expo config. HTTP is for local development only. Native preproduction/production builds reject missing or non-HTTPS public URLs. The Android emulator can use `http://10.0.2.2:4000`.

## Architecture

- `apps/mobile`: Expo SDK 55 / React Native 0.83, Expo Router, native camera, on-device OCR, SecureStore, TanStack Query and a bounded persisted product cache.
- `apps/api`: Fastify, parameterized PostgreSQL queries, versioned transactional SQL migrations, Argon2id, short-lived JWTs and hashed rotating refresh sessions.
- `apps/web`: Next.js public viewers, Open Graph metadata, privacy/methodology and association endpoints.
- `packages/contracts`: shared Zod schemas; `packages/gluten-engine` and `packages/nutrition-engine`: pure deterministic TypeScript.

Each deployed environment has one API process, one private PostgreSQL database and one Next.js process. Production and preproduction use separate Docker Compose projects, volumes and secrets. No Redis. Provider budgets are process-local, so do not scale API replicas without implementing a shared provider rate budget.

## Validation

```sh
npm run lint
npm run format:check
npm run typecheck
npm test
# Point this at an isolated test database, never production:
TEST_DATABASE_URL=postgres://.../safeeat_test npm run test:integration
npm run build
npm run mobile:check
cd apps/mobile
npx expo-doctor
npx expo export --platform ios --platform android
```

PowerShell: `$env:TEST_DATABASE_URL='postgres://.../safeeat_test'`, then `npm run test:integration`. Integration tests run real migrations and check auth, replay revocation, IDOR, isolation, favorites and public sharing. `maestro test -e TEST_EMAIL=... -e TEST_PASSWORD=... -e TEST_BARCODE=... .maestro/smoke.yml` runs an optional installed-device smoke flow; use a disposable account and a known provider barcode.

## Providers and safety

[Open Food Facts](https://openfoodfacts.github.io/openfoodfacts-server/api/) uses product API v3 and the official [Search-a-licious API](https://openfoodfacts.github.io/search-a-licious/users/ref-openapi/). Search-index summaries are marked stale, carry no safety/nutrition claims and never overwrite full product data; opening a summary fetches its current product record. Requests identify SafeEat, use an 8-second timeout and enforce 14 product reads / 8 searches per minute. Full products are cached for 24 hours. A stale record may be returned on provider failure, explicitly marked stale. A 404, network failure and provider failure are distinct. Search input is debounced and cached server-side. Production operators should register their app with OFF and set a contact in `OPENFOODFACTS_USER_AGENT`.

OFF data is community-supplied (ODbL), contents DbCL, images CC BY-SA. Provider data stays separate from personal labels. Preserve attribution and applicable share-alike obligations when redistributing the cached database.

OCR uses [expo-text-extractor](https://github.com/pchalupa/expo-text-extractor), Apple Vision on iOS and bundled ML Kit on Android. Personal label photos stay on device. For a scanned barcode, an authenticated user can explicitly contribute a reviewed ingredients photo and text; the server re-encodes the JPEG without metadata and makes it available to other users. Community evidence is labeled separately and never certifies a product. The engine does not infer safety from packaging appearance. See [methodology](docs/SAFETY.md).

When `USDA_FDC_API_KEY` is configured on the server, an inconclusive barcode lookup can also consult [USDA FoodData Central](https://fdc.nal.usda.gov/api-guide/) by exact GTIN. This key is optional and never bundled into the app. Missing evidence remains inconclusive.

Restaurants use OpenStreetMap through [Photon](https://github.com/komoot/photon) and [Overpass](https://wiki.openstreetmap.org/wiki/Overpass_API) when no Google key is configured. Public endpoints are rate-limited, cached and intended for modest traffic; a self-hosted or contracted provider is needed at scale. `GOOGLE_PLACES_API_KEY` switches to [Google Places API (New)](https://developers.google.com/maps/documentation/places/web-service/overview). Names and reviews never establish gluten safety. Only provider IDs persist in restaurant favorites and shares; details are fetched on demand. A privacy policy and operator identity must be completed before store distribution.

The restaurant screen shows an interactive map with pins from the restaurant search. iOS uses Apple Maps without an API key. Android uses Google Maps and requires a Google Cloud project with billing and Maps SDK for Android enabled. Set `EXPO_PUBLIC_GOOGLE_MAPS_ANDROID_API_KEY` for native Android builds (GitHub environment variable `GOOGLE_MAPS_ANDROID_API_KEY`); restrict that key to package `com.a1var0w.safeeat`, the signing certificate SHA-1, and Maps SDK for Android. The key is embedded in the app, so it must be app-restricted. A new APK/IPA is required after adding the native map module. The server-side `GOOGLE_PLACES_API_KEY` is separate and optional; it supplies restaurant data, not map tiles. To use Google's map tiles on iOS as well, replace the iOS map implementation and configure Maps SDK for iOS with its own app-restricted key.

## Authentication and privacy

Access tokens expire in 10 minutes. Refresh credentials rotate, expire after an absolute 30 days, and are stored only as HMAC hashes server-side. Reuse revokes the whole session family. Protected requests check live session revocation. Logout must reach the server to confirm revocation. Account deletion cascades personal labels, contributed ingredient photos, saved groups, favorites, sessions and private-label shares. SecureStore holds the refresh credential and user identity; access JWTs stay in memory. No auth tokens in AsyncStorage or logs. Clear device caches at account changes.

Public share tokens have 192 bits of randomness. Public provider shares carry no user identity. A personal label is private until its owner explicitly shares it. Anyone with that link can read its text. Account deletion revokes personal-label links; backup retention is 14 days.

## Android and iOS builders

`.github/workflows/android-builder.yml` publishes only the installable `SafeEat-<version>-production.apk` or preproduction APK. Signature, package, permissions, embedded configuration and archive integrity are still checked during the build. Both current public-testing channels use Expo's testing signing key; they are installable, not for Play publication. Enable `production_signing` and provide `ANDROID_KEYSTORE_BASE64`, `ANDROID_KEYSTORE_PASSWORD`, `ANDROID_KEY_ALIAS`, `ANDROID_KEY_PASSWORD` in the selected GitHub Environment for final signing. Keystores are never committed.

`.github/workflows/ios-builder.yml` publishes only the installable `SafeEat-<version>-production.ipa` or preproduction IPA, with `Payload/SafeEat.app`, using `CODE_SIGNING_ALLOWED=NO`. This requires later signing (for example through SideStore); it is not an App Store archive. Version comes from the root package; build number from the workflow run. Both channels retain bundle/package ID `com.a1var0w.safeeat`, so install one channel at a time.

For the current 0.2.1 iOS build, add `https://chat-ink.tail552c89.ts.net:10000/safeeat-downloads/safeeat.json` as a source in SideStore or LiveContainer. Preproduction is `https://chat-ink.tail552c89.ts.net:10000/safeeat-downloads/preproduction.json`. The source uses the classic AltStore app fields accepted by both clients; the IPA is unsigned and is signed on device. LiveContainer's deep-link form is `livecontainer://sources?url=<encoded source URL>`, while SideStore uses `sidestore://source?url=<encoded source URL>`. The same bundle identifier means the two channels cannot be installed side by side.

Both workflows use `workflow_dispatch`/`workflow_call` with `target_environment`, lockfile installation, native prebuild and artifact validation. They explicitly set `APP_ENV` and read `API_PUBLIC_URL` / `PUBLIC_APP_URL` from GitHub Environments. Release configuration rejects missing, local, IP or non-HTTPS URLs. A public preflight checks the API's actual environment and web health; the final APK/IPA is inspected to confirm its embedded URLs and standalone JavaScript bundle. There is no Metro dependency. See [deployment](infrastructure/production/README.md) and [executed validation and artifact links](docs/VALIDATION.md).

The installed-app connectivity failure was caused by `AbortSignal.timeout`, which the React Native abort-controller implementation does not provide. Requests now use an ordinary AbortController plus a cleared timer, with regression coverage for that native limitation. Settings provides a connection check showing the server environment/version; offline, timeout, secure-connection failure, HTTP and authentication errors remain distinct.

## Sharing and deep links

Configure `PUBLIC_APP_URL` once. Production uses the root origin; the isolated preproduction deployment uses `/preproduction`. API endpoints append `/api/v1` to the environment's base URL.

- `https://<host>/p/<token>` and `/r/<token>` work without login.
- iOS: set `APPLE_TEAM_ID` on web, sign with the matching team and associated-domain entitlement. Unsigned IPA alone cannot establish Universal Links.
- Android: set comma-separated signing-certificate SHA-256 fingerprints in `ANDROID_SHA256_FINGERPRINTS` on web, including the test cert if testing verified links.
- Both association files are served at `/.well-known/`, without redirects. Empty unconfigured files deliberately authorize no application.
- Fallback: `safeeat://p/<token>`, `safeeat://r/<token>`.

## Deployment

Current status: **PROVISIONAL PRODUCTION / PUBLIC TESTING** through persistent Tailscale Funnel. These URLs are publicly reachable without a Tailnet account:

| Environment   | Web / API base URL                                       | Storage                               |
| ------------- | -------------------------------------------------------- | ------------------------------------- |
| Development   | `http://localhost:3000` / `http://localhost:4000`        | Local development Compose             |
| Preproduction | `https://chat-ink.tail552c89.ts.net:10000/preproduction` | `safeeat-preproduction_postgres-data` |
| Production    | `https://chat-ink.tail552c89.ts.net:10000`               | `safeeat_postgres-data`               |

Production API: [metadata](https://chat-ink.tail552c89.ts.net:10000/api/v1/meta), with endpoints under `/api/v1`. Use `bash scripts/deploy-server.sh` for production or supply the absolute `preproduction.env` path for preproduction; read [the server guide](infrastructure/production/README.md). Both databases are private and persistent; API/web bind only to loopback. DuckDNS production remains pending external WAN ingress/TLS setup. Existing Chat-Ink routes are preserved.
