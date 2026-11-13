# SAFE EAT — INFORME GO / NO-GO PARA PRODUCCIÓN

**Versión candidata:** SafeEat 2.0.5, commit `b93bbc2f57bd`, rama `claude/safeeat-2.0.5`.
**Fecha:** 13-11-2026.
**Estado de producción:** sin tocar. Sigue la API 1.x de Node con el esquema en la migración 009. `releases/latest.json` no se ha publicado.

## Recomendación

**GO técnico del servidor; NO-GO todavía para publicar la app.**

- El servidor 2.0.5 y la migración de la base de datos están probados sobre una copia real de producción, y la vuelta atrás está comprobada.
- Faltan tus pruebas en los móviles físicos (Android e iPhone). No las doy por superadas. Hasta que me pases los resultados, no recomiendo publicar el APK ni el IPA de producción.
- **No haré nada en producción sin tu autorización expresa.**

## Qué está verificado (ejecutado por mí)

| Comprobación | Resultado |
| --- | --- |
| `cargo fmt`, `clippy -D warnings`, `cargo test` (servidor y app), clippy de la UI para WASM, `trunk build --release` | Correcto |
| `npm` lint, format, typecheck y test; `check-workflows` | Correcto |
| CI de GitHub en la rama (run 43441153696) | Correcto |
| APK e IPA de preproducción | Publicados en el canal de preproducción (enlaces en [release-2.0.5.md](release-2.0.5.md)); producción sin tocar |
| Prueba de extremo a extremo de la API (`scripts/smoke-api.mjs`) contra PostGIS desechable | 59/59 |
| Motor de gluten 1.2.3: evaluación | 0/83 falsos negativos peligrosos, 0/60 falsos positivos, cobertura 484/499 |
| Migraciones 010–018 sobre la copia de seguridad de producción del 13-11-2026, en una base aislada | 10 s; usuarios, contraseñas, roles, sesiones, favoritos, reseñas, compartidos y auditoría idénticos byte a byte |
| Vuelta atrás: API 1.x sobre el esquema migrado, y la 2.0.5 leyendo lo que escribe la 1.x con sus tokens (`scripts/check-rollback.mjs`) | Correcto |
| Restauración de copias (`scripts/verify-backup-restore.sh`): la de producción y una del esquema migrado | Integridad correcta |
| Cobertura de restaurantes con los límites reales de Geofabrik (base local) | Cubiertas las 9 ciudades del encargo, zonas rurales, Baleares, Ceuta, Melilla y Canarias; Lisboa, Elvas, Toulouse y Marrakech fuera |
| Despliegue en preproducción | Correcto: migraciones 015–018, servicios sanos, `meta` responde por el Funnel |
| Restaurantes en preproducción: `spain` (153 698) y `canarias` (9 726) | Cubiertas las 9 ciudades del encargo, una zona rural, Canarias y Palma; Lisboa no |
| Productos de la regresión en preproducción | Kimbo 8410044005008: no verde (sin gluten identificado, sin confirmar). Chic-kles 8402001044311: «información insuficiente» con texto mezclado apartado |

## Qué NO está verificado

- **Pruebas en tus móviles físicos:** toda la lista de [release-2.0.5.md](release-2.0.5.md). Pendiente de tus resultados.
- **OCR en un móvil real:** velocidad de la primera lectura y uso de memoria. Solo lo he probado en el navegador del ordenador.
- **Tasa de falsos negativos inferior al 0,01 %:** no está demostrada. Harían falta al menos 29 956 positivos etiquetados de forma independiente.
- **La app Expo 1.0.12/1.0.13 contra la API en Rust en producción:** el contrato es el mismo y se probó en fases anteriores, pero no en esta.
- **`cargo audit`:** no está instalado; la revisión de seguridad fue de código.

## Plan para producción (solo con tu autorización)

1. **Copia de seguridad** verificada antes de tocar nada (`deploy-remote.sh --env production` la hace primero y para si falla).
2. **Despliegue:** `scripts/deploy-remote.sh --env production`. Pide escribir `production` para confirmar. Aplica las migraciones 010–018 (unos 10 s con los datos de hoy), arranca la API y el visor, y comprueba `meta` por el Funnel. Si algo falla, vuelve sola a los contenedores anteriores.
3. **Comprobaciones posteriores:** login con una cuenta real, favoritos, un escaneo y la salud de HallowFest (ya daba 502 antes de este trabajo; no lo he causado ni tocado).
4. **Catálogo de restaurantes:** importar `spain` y `canarias` con el entorno de producción, con `TMPDIR` en disco (el `/tmp` del servidor es un tmpfs de 3,4 GB). En preproducción tardó unos 10 minutos en total, con la descarga de 1,4 GB incluida.
5. **Publicar la app** (`latest.json` y el manifiesto de SideStore) **solo** después de tus pruebas en los móviles y con una segunda autorización.

### Vuelta atrás

- **Si falla el arranque:** el script vuelve solo a la imagen anterior.
- **Si aparece un problema después:** volver a la imagen 1.x de Node. Funciona sobre el esquema migrado (comprobado), porque las migraciones son solo aditivas. No hace falta restaurar la base.
- **Nunca** se restaura automáticamente una copia antigua encima de la base de producción. Si hiciera falta, sería una decisión tuya, con la copia del paso 1.

## Riesgos

| Riesgo | Mitigación |
| --- | --- |
| Primer paso de producción de Node a Rust | Mismo contrato, prueba de vuelta atrás, copia previa y vuelta automática si falla `meta` |
| El OCR puede ser lento o leer mal en móviles modestos | El texto siempre se puede corregir; sin la comprobación marcada el resultado nunca es favorable |
| Importar toda España ocupa disco y tiempo | Se hace con prioridad baja (`nice`), fuera del contenedor de la base, con `TMPDIR` en disco |
| `meta` indica la versión 2.0.0 | Solo es la etiqueta del workspace raíz; la app sí dice 2.0.5. Cosmético |
| La API 1.x tiene avisos de `npm audit` conocidos | Solo se usaría para volver atrás |

## Lo que necesito de ti

1. Los resultados de la lista de pruebas en Android e iPhone.
2. Una autorización expresa para el despliegue en producción, por ejemplo: «Autorizo desplegar 2.0.5 en PRO».
3. Después, otra autorización aparte para publicar el APK y el IPA de producción.
