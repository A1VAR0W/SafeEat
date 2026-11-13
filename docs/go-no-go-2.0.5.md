# SAFE EAT — INFORME GO / NO-GO PARA PRODUCCIÓN

**Versión candidata:** SafeEat 2.0.5, rama `claude/safeeat-2.0.5`.

- Servidor: commit `b93bbc2f57bd`, desplegado en preproducción.
- App: compilaciones de preproducción del commit `bd303d9`, con el arreglo del lector de fotos.

**Fecha:** 13-11-2026.

**Estado de producción:** sin tocar.

- Sigue la API 1.x de Node, con el esquema en la migración 009.
- `releases/latest.json`, `production.json` y `safeeat.json` siguen con fecha del 6 de octubre.

## Recomendación

**NO-GO por ahora. Pasa a GO condicionado con dos cosas:**

1. **Tus pruebas en los móviles físicos** (Android e iPhone) con las compilaciones de preproducción, sobre todo la foto de ingredientes. No las doy por superadas.
2. **Tu autorización expresa.** No haré nada en producción sin ella.

**Lo que ya no bloquea: la app de producción (Expo 1.0.12) contra el servidor en Rust.**

- Al desplegar el servidor 2.0.5 en producción, esa app seguirá usándose contra él hasta que se publique la 2.0.5.
- Lo he probado hoy en el emulador contra preproducción: búsqueda, ficha de producto, restaurantes, inicio de sesión y favoritos funcionan.
- No he probado la cámara del escáner.

**Por qué no es un GO directo:** hoy mismo, en el emulador, la foto de ingredientes **no funcionaba dentro del APK**, aunque sí en el navegador (detalles abajo).

- Ya está corregido y comprobado en el emulador.
- Demuestra que lo que no se prueba en un móvil puede fallar, y en el iPhone todavía no se ha probado.

## Qué está verificado (ejecutado por mí)

| Comprobación                                                                                                                              | Resultado                                                                                                                  |
| ----------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------- |
| `cargo fmt`, `clippy -D warnings`, `cargo test` (servidor y app), clippy de la UI para WASM, `trunk build --release`                      | Correcto                                                                                                                   |
| `npm` lint, format, typecheck y test; `check-workflows`                                                                                   | Correcto                                                                                                                   |
| CI de GitHub en la rama                                                                                                                   | Ver [release-2.0.5.md](release-2.0.5.md)                                                                                   |
| Prueba de extremo a extremo de la API (`scripts/smoke-api.mjs`) contra PostGIS desechable                                                 | 59/59                                                                                                                      |
| Motor de gluten 1.2.3: evaluación                                                                                                         | 0/83 falsos negativos peligrosos, 0/60 falsos positivos, cobertura 484/499                                                 |
| Migraciones 010–018 sobre la copia de seguridad de producción del 13-11-2026, en una base aislada                                         | 10 s. Usuarios, contraseñas, roles, sesiones, favoritos, reseñas, compartidos y auditoría quedan idénticos byte a byte     |
| Vuelta atrás: la API 1.x sobre el esquema migrado, y la 2.0.5 leyendo lo que escribe la 1.x con sus tokens (`scripts/check-rollback.mjs`) | Correcto. Prueba la API 1.x, no la app Expo                                                                                |
| Restauración de copias (`scripts/verify-backup-restore.sh`): la de producción y una del esquema migrado                                   | Integridad correcta                                                                                                        |
| Despliegue en preproducción                                                                                                               | Correcto: migraciones 015–018, servicios sanos y `meta` responde por el Funnel                                             |
| Restaurantes en preproducción: `spain` (153 698) y `canarias` (9 726)                                                                     | Cubiertas las 9 ciudades del encargo, dos zonas rurales, Canarias y Palma. Lisboa no                                       |
| Productos de la regresión en preproducción                                                                                                | Kimbo 8410044005008: no sale en verde. Chic-kles 8402001044311: «información insuficiente», con el texto mezclado apartado |
| APK 2.0.5 (versionCode 2000044) instalado encima de la 2.0.4 en el emulador                                                               | Se instala sin desinstalar. La foto de una etiqueta se lee bien                                                            |
| App de producción (Expo 1.0.12) contra el servidor 2.0.5, en el emulador                                                                  | Búsqueda, ficha, restaurantes, inicio de sesión y favorito correctos. Cuenta de prueba borrada                             |

### El lector de fotos dentro de la app

- **Primer APK (versionCode 2000043), en el emulador:** con una foto clara de una etiqueta, «No se ha podido leer la foto». Falla de forma segura (no deja analizar nada), pero la función no servía.
- **Causa probable, sin confirmar:** el APK de producción no deja inspeccionar su WebView, así que no vi el error exacto. Hay dos candidatos:
  - El motor de Tesseract carga su parte en WebAssembly con `fetch(data:)`, y la política de la app (`connect-src`) no lo permitía. Lo vi bloqueado en el navegador con esa política.
  - En Android, las peticiones propias de un worker podrían no pasar por el servidor de archivos de la app. No lo he comprobado.
- **Lo que sí está demostrado:** con el arreglo, la foto se lee dentro del APK en el emulador.
- **Arreglo (commit `bd303d9`):**
  - La página lee el worker, el motor y el modelo de español, y se los pasa al worker como `blob:` y como bytes. El worker ya no pide nada.
  - La política permite `blob:` en los scripts y `data:` en las conexiones. Las dos cosas son locales: no abren nada nuevo en la red.
  - Hay un fallo de una línea en Tesseract.js 5.1.1, corregido y anotado en `NOTICE.txt`.
- **Probado en el navegador con esa misma política:** texto perfecto, confianza 95, 0,6 s y ninguna violación.
- **En el emulador, con el APK nuevo (versionCode 2000044):** la misma foto se lee sin errores y deja corregir el texto.

## Qué NO está verificado

- **Pruebas en tus móviles físicos:** toda la lista de [release-2.0.5.md](release-2.0.5.md). Pendiente de tus resultados.
- **El lector de fotos en iPhone:** nunca se ha ejecutado dentro de la app de iOS. Es la parte con más riesgo de la lista del iPhone. Depende de dos cosas propias de WebKit que no puedo probar desde aquí:
  - `importScripts` de una URL `blob:` con el fragmento `#core.js`.
  - Un worker `blob:` bajo `tauri://`.
  - Si falla, el siguiente paso es meter el motor y el worker en un solo `blob:`, para que el worker no tenga que cargar nada.
- **La cámara del escáner** de la app de producción contra Rust: el emulador no tiene cámara. La búsqueda del producto que hace después sí está probada.
- **Tasa de falsos negativos inferior al 0,01 %:** no está demostrada. Harían falta al menos 29 956 positivos etiquetados de forma independiente.
- **La vuelta atrás automática al pasar de Node a Rust:** el mecanismo funciona en preproducción entre versiones en Rust, pero no lo he ejecutado al cambiar de Node a Rust, que usa otro fichero de compose.
- **`cargo audit`:** no está instalado; la revisión de seguridad fue de código.

## Plan para producción (solo con tu autorización)

1. **Proteger la vuelta atrás:** volver a etiquetar las imágenes actuales (`safeeat-api:local` y `safeeat-web:local`) con un nombre fijo, para que ninguna compilación pueda pisarlas.
2. **Copia de seguridad** verificada antes de tocar nada. `deploy-remote.sh --env production` la hace primero y para si falla.
3. **Despliegue** con `scripts/deploy-remote.sh --env production`:
   - Pide escribir `production` para confirmar.
   - Aplica las migraciones 010–018 (unos 10 s con los datos de hoy).
   - Arranca la API y el visor, y comprueba `meta` por el Funnel.
   - Si algo falla, vuelve sola a los contenedores anteriores.
4. **Comprobaciones posteriores:**
   - Login con una cuenta real, favoritos y un escaneo con la app Expo actual.
   - La salud de HallowFest. Ya daba 502 antes de este trabajo; no lo he causado ni tocado.
5. **Catálogo de restaurantes:** importar `spain` y `canarias` con el entorno de producción. El script ya trabaja en disco (`/var/tmp`) y no en el `/tmp` en RAM. En preproducción tardó unos 10 minutos, con la descarga de 1,4 GB incluida.
   - El temporizador semanal de importación **no está instalado** en el servidor. Si se quiere, sería otro paso con tu permiso.
6. **Publicar la app** (`latest.json` y el manifiesto de SideStore) **solo** después de tus pruebas en los móviles y con una segunda autorización.

### Vuelta atrás

- **Si falla el arranque:** el script vuelve solo a la imagen anterior.
- **Si aparece un problema después:** volver a la imagen 1.x de Node (la del paso 1).
  - Funciona sobre el esquema migrado (comprobado), porque las migraciones son solo aditivas.
  - No hace falta restaurar la base.
- **Nunca** se restaura automáticamente una copia antigua encima de la base de producción. Si hiciera falta, sería una decisión tuya, con la copia del paso 2.

## Riesgos

| Riesgo                                                                  | Mitigación                                                                                                                                                                                                                                                                                                                                                     |
| ----------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Primer paso de producción de Node a Rust                                | Mismo contrato, prueba de vuelta atrás de la API, imágenes antiguas protegidas, copia previa y vuelta automática si falla `meta`                                                                                                                                                                                                                               |
| Las apps antiguas (Expo 1.x y 2.0.4) ignoran `provenance` y `freshness` | Ejemplo: los chicles 8480000651471 salen en verde, «SIN GLUTEN DECLARADO», por un dato de la comunidad de Open Food Facts. La API 1.x que está hoy en producción da lo mismo (comprobado en local con la copia de producción), así que no empeora. Solo mejora al instalar la 2.0.5, que lo muestra en azul como «según datos de la comunidad · sin verificar» |
| El lector de fotos puede fallar o ir lento en algunos móviles           | Si falla, no deja analizar nada. El texto siempre se puede corregir, y sin la comprobación marcada el resultado nunca es favorable                                                                                                                                                                                                                             |
| Importar toda España ocupa disco y tiempo                               | Prioridad baja (`nice`), en disco y fuera del contenedor de la base                                                                                                                                                                                                                                                                                            |
| `meta` indica la versión 2.0.0                                          | Solo es la etiqueta del workspace raíz; la app sí dice 2.0.5. Cosmético                                                                                                                                                                                                                                                                                        |
| La API 1.x tiene avisos de `npm audit` conocidos                        | Solo se usaría para volver atrás                                                                                                                                                                                                                                                                                                                               |

## Lo que necesito de ti

1. Los resultados de la lista de pruebas en Android e iPhone.
2. Una autorización expresa para el despliegue en producción, por ejemplo: «Autorizo desplegar 2.0.5 en PRO».
3. Después, otra autorización aparte para publicar el APK y el IPA de producción.
