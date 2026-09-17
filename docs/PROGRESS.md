# SafeEat progress — 2026-09-17

Continuation from clean `main` at `072e400`; existing implementation and unrelated server services preserved.

- Fixed Android SDK setup (`tools` was retired) and formatting CI failures. Expo Router resolves and exports both native bundles.
- Fixed shared private-label detail navigation, favorite membership after page 1, ingredient-camera lifecycle, bodyless POST/DELETE requests, OCR draft preservation through login, refresh/account-change races and private-cache clearing while retaining public offline products. Detail cache bounded to 100 inactive entries.
- Hardened release URL validation, gluten claim interpretation, concurrent refresh-family revocation and HTTP 429 handling. Added regression coverage.
- Replaced unavailable legacy OFF text search with the official Search-a-licious service. Search summaries never establish gluten freedom or overwrite full product evidence; detail requests refresh through the product API.
- Healthy isolated Docker deployment at `/opt/safeeat`. Public TLS ingress: `https://chat-ink.tail552c89.ts.net:10000`; existing private ports 9443/9444 and Chat-Ink Funnel ports 443/8443 preserved.
- PostgreSQL is private on a persistent volume. Backup created, inspected and restored into an isolated database; restore copy removed after verification.
- Real HTTPS flow checked: register/login/refresh/logout, reviewed text, favorites, private-label isolation, anonymous public viewer/OG, account deletion and share revocation. Live OFF barcode and search both pass after deploying the new adapter.
- 93 unit tests and 20 real PostgreSQL integration tests pass. Lint, formatting, typecheck, builds, Expo Doctor and native exports pass. Native builds and installed Android smoke are tracked in `VALIDATION.md`.

External requirements: Google Places key; router/NAT/TLS setup for DuckDNS; Apple team/signing and canonical port-443 domain plus Android certificate associations for verified links. Native hardware camera/OCR and iOS installation require device validation.
