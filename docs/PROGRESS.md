# SafeEat progress — 2026-09-18

## V2 in progress

The V2 product and restaurant experience is complete for private production use. Product grids share a persisted 1/2/3-column selector, ingredient photos and user gluten reports are stored separately from source data, and restaurants support map movement, autocomplete, precise location, community additions and gluten reports. Android uses MapLibre/OpenFreeMap and iOS uses Apple Maps; neither needs Google Cloud.

Continuation from clean `main` at `072e400`; existing implementation and unrelated server services preserved.

- Fixed Android SDK setup (`tools` was retired) and formatting CI failures. Expo Router resolves and exports both native bundles.
- Fixed shared private-label detail navigation, favorite membership after page 1, ingredient-camera lifecycle, bodyless POST/DELETE requests, OCR draft preservation through login, refresh/account-change races and private-cache clearing while retaining public offline products. Detail cache bounded to 100 inactive entries.
- Hardened release URL validation, gluten claim interpretation, concurrent refresh-family revocation and HTTP 429 handling. Added regression coverage.
- Replaced unavailable legacy OFF text search with the official Search-a-licious service. Search summaries never establish gluten freedom or overwrite full product evidence; detail requests refresh through the product API.
- Diagnosed installed-app Internet failure: React Native lacks `AbortSignal.timeout`. Replaced it with an AbortController/timer and regression tests; added controlled connection diagnostics and environment/version verification. Build-time validation and final-archive inspection reject missing/incorrect release targets.
- **PROVISIONAL PRODUCTION / PUBLIC TESTING** at `https://chat-ink.tail552c89.ts.net:10000`; separate preproduction at `/preproduction`, with independent PostgreSQL volumes, JWT/database secrets, Compose projects and API/web ports. Existing private ports 9443/9444 and Chat-Ink Funnel ports 443/8443 preserved.
- PostgreSQL is private on a persistent volume. Backup created, inspected and restored into an isolated database; restore copy removed after verification.
- Real HTTPS flow checked: register/login/refresh/logout, reviewed text, favorites, private-label isolation, anonymous public viewer/OG, account deletion and share revocation. Live OFF barcode and search both pass after deploying the new adapter.
- Both public environments pass auth/product/favorite/share/account-cleanup flows; cross-environment access/refresh tokens are rejected. Latest production dump restored into a separate temporary PostgreSQL container; test containers and SSH tunnel removed.
- 110 unit tests and 20 real PostgreSQL integration tests pass. Lint, formatting, typecheck, builds, Expo Doctor and native exports pass. Environment-specific APK/unsigned IPA builds and the complete installed Android Internet test are tracked in `VALIDATION.md`.
- Shared HTTPS links respect each environment's URL prefix; native routing removes the preproduction prefix and rejects links for the wrong environment. Final Android/iOS artifacts from `9dfd4fb` passed builds and all downloaded checksums; installed production run `35194870325` passed all nine Internet checks without Metro, including persisted favorites and native sharing. Disposable account removed. Scheduled backup also completed successfully.

External requirements: router/NAT/TLS setup for DuckDNS; Apple team/signing and canonical port-443 domain plus Android certificate associations for verified links. Native hardware camera/OCR and iOS installation require device validation. OpenStreetMap's public services are appropriate for this private deployment but need a managed or self-hosted provider before public scale.
