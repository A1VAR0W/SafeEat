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

One API process, one PostgreSQL database, one Next.js process. No Redis. Provider budgets are process-local, so do not scale API replicas without implementing a shared provider rate budget.

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

[Open Food Facts](https://openfoodfacts.github.io/openfoodfacts-server/api/) uses product API v3 and the documented legacy full-text search endpoint. Requests identify SafeEat, use an 8-second timeout and enforce 14 product reads / 8 searches per minute, under published limits. Products are cached for 24 hours. A stale record may be returned on provider failure, explicitly marked stale. A 404, network failure and provider failure are distinct. Searches require submission; no search-as-you-type. Production operators should register their app with OFF and set a contact in `OPENFOODFACTS_USER_AGENT`.

OFF data is community-supplied (ODbL), contents DbCL, images CC BY-SA. Provider data stays separate from personal labels. Preserve attribution and applicable share-alike obligations when redistributing the cached database.

OCR uses [expo-text-extractor](https://github.com/pchalupa/expo-text-extractor), Apple Vision on iOS and bundled ML Kit on Android. No frame-by-frame OCR or photo uploads. Text can be corrected; unreviewed OCR never establishes a gluten-free claim. Saving sends only the reviewed text. The engine does not certify products or infer safety from packaging appearance. See [methodology](docs/SAFETY.md).

Restaurants use [Google Places API (New)](https://developers.google.com/maps/documentation/places/web-service/overview). Set server-only `GOOGLE_PLACES_API_KEY`, enable Places API and billing, and restrict the key to the server/API. Without it the app explains that the provider is not configured. Names and reviews never establish gluten safety. Only place IDs persist in restaurant favorites and shares; Google details are fetched on demand, attributed, and excluded from disk caches. A privacy policy and operator identity must be completed before store distribution.

## Authentication and privacy

Access tokens expire in 10 minutes. Refresh credentials rotate, expire after an absolute 30 days, and are stored only as HMAC hashes server-side. Reuse revokes the whole session family. Protected requests check live session revocation. Logout must reach the server to confirm revocation. Account deletion cascades personal labels, favorites, sessions and private-label shares. SecureStore holds the refresh credential and user identity; access JWTs stay in memory. No auth tokens in AsyncStorage or logs. Clear device caches at account changes.

Public share tokens have 192 bits of randomness. Public provider shares carry no user identity. A personal label is private until its owner explicitly shares it. Anyone with that link can read its text. Account deletion revokes personal-label links; backup retention is 14 days.

## Android and iOS builders

`.github/workflows/android-builder.yml` creates `SafeEat-0.1.0.apk`, `.aab` and SHA-256 files. Preproduction uses Expo's testing signing key; it is installable, not for Play publication. Enable `production_signing` and provide `ANDROID_KEYSTORE_BASE64`, `ANDROID_KEYSTORE_PASSWORD`, `ANDROID_KEY_ALIAS`, `ANDROID_KEY_PASSWORD` in the selected GitHub Environment for production signing. Keystores are never committed.

`.github/workflows/ios-builder.yml` creates `SafeEat-0.1.0.ipa` and SHA-256, with `Payload/SafeEat.app`, using `CODE_SIGNING_ALLOWED=NO`. This requires later signing (for example through SideStore); it is not an App Store archive. App Store distribution is separate. Version comes from the root package; build number from the workflow run.

Both workflows use `workflow_dispatch`/`workflow_call`, lockfile installation, native prebuild and artifact validation. Configure `API_PUBLIC_URL` and `PUBLIC_APP_URL` in GitHub Environments `preproduction` / `production`. They must be HTTPS. See [deployment](infrastructure/production/README.md) and [validation status](docs/VALIDATION.md).

## Sharing and deep links

Configure `PUBLIC_APP_URL` once, without a path. API can share the same HTTPS host under `/api/v1`.

- `https://<host>/p/<token>` and `/r/<token>` work without login.
- iOS: set `APPLE_TEAM_ID` on web, sign with the matching team and associated-domain entitlement. Unsigned IPA alone cannot establish Universal Links.
- Android: set comma-separated signing-certificate SHA-256 fingerprints in `ANDROID_SHA256_FINGERPRINTS` on web, including the test cert if testing verified links.
- Both association files are served at `/.well-known/`, without redirects. Empty unconfigured files deliberately authorize no application.
- Fallback: `safeeat://p/<token>`, `safeeat://r/<token>`.

## Deployment

Use `bash scripts/deploy-server.sh` in an isolated checkout; read [the server guide](infrastructure/production/README.md). PostgreSQL has no published production port; API and web bind only to loopback. Nginx terminates HTTPS. Do not expose local development servers or PostgreSQL. Never replace unrelated infrastructure.
