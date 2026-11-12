# SafeEat 2.0: estado (noche del 12-11-2026)

Rama: `claude/safeeat-2.0-rust` (subida a GitHub, sin fusionar con `main`).
**Producción no se ha tocado.** Solo está desplegada la **preproducción** (`:10000/preproduction`).
Tampoco se ha publicado `latest.json` ni los manifiestos de SideStore.
HallowFest ha devuelto 200 antes y después de cada despliegue.

## Qué hay desplegado en preproducción

### Servidor en Rust (axum)
- Sustituye a la API de Node y al visor web de Next.js con el mismo contrato, así que la app 1.0.12/1.0.13 funciona igual.
- La batería `scripts/smoke-api.mjs` pasa las 41 comprobaciones en local.
- Al desplegar se comprueba a través del Funnel; si algo falla, vuelve solo a la versión anterior.

### Ingredientes en español
- Cuando Open Food Facts solo tiene la lista en otro idioma, el servidor la traduce con los nombres oficiales en español de su taxonomía. Por ejemplo, `en:wheat-flour` pasa a «harina de trigo».
- No se usa ninguna IA para traducir.
- Los nombres traducidos se guardan en la base de datos durante 90 días.
- Si se puede traducir menos del 60 % de la lista, se muestra la original para no mezclar idiomas.
- Se guarda la lista original (`ingredientsOriginal`) y su idioma (`ingredientsLanguage`), para el botón «ver original».
- **Seguridad:**
  - El análisis de gluten se hace siempre sobre la lista original.
  - La traducción solo puede hacer el resultado **más prudente**. Ejemplo: «Farine de blé» sola es DESCONOCIDO y traducida pasa a CONTIENE GLUTEN.
  - Nunca puede convertir un producto en «sin gluten».

### «Verificado por IA» (verde con aviso)
- **Modelo:** Qwen2.5 1.5B (Q4_K_M) con llama.cpp, en un contenedor `ai` del propio servidor.
- **Límites del contenedor:**
  - Memoria: 1,4 GB como máximo (en reposo usa unos 170 MB).
  - CPU: 1,5 núcleos.
  - Sin acceso a internet, en solo lectura y sin privilegios.
- **Por qué es tan pequeño:** el servidor tiene un AMD E2-6110 sin AVX2 y 6,7 GB de RAM.
  - Lee unos 6 tokens/s y genera unos 2,5 tokens/s.
  - Un modelo mayor dejaría sin recursos a HallowFest.
- **Lo que se midió con 50 listas reales** (30 con gluten y 20 sin gluten), contando cuántas veces dijo «sin gluten» a una lista que sí lo tenía:

| Cómo se le preguntó | Errores en las 30 listas con gluten |
|---|---|
| Lista completa, modelo 0.5B | 22 |
| Lista completa, modelo 1.5B | 28 |
| Ingrediente a ingrediente | 2 (seitán, escanda) |
| Ingrediente a ingrediente, con ejemplos | 1 (bulgur) |

  **Conclusión:** el modelo solo no es fiable para una persona celíaca.
- **Cómo se ha montado:** la IA da una segunda opinión y nunca decide sola. Solo se le pregunta si se cumplen todas estas condiciones:
  1. El motor determinista no ha encontrado nada (estado DESCONOCIDO).
  2. Open Food Facts reconoce todos los ingredientes.
  3. Ningún ingrediente aparece en la lista negra multilingüe (trigo, cebada, centeno, avena, espelta, kamut, seitán, bulgur, malta, Weizen…, etc.).
  4. La lista tiene entre 1 y 40 ingredientes cortos.
- **Comprobación de las condiciones:** el test `gate_regression` demuestra que ninguna de las 30 listas con gluten llega al modelo.
- **Resultados posibles:**
  - **Si el modelo dice «no» a cada ingrediente:** el estado es `VERIFIED`. El aviso visible dice que la IA puede equivocarse y que no detecta trazas ni contaminación cruzada.
  - **Si duda de algún ingrediente:** el estado es `SUSPECTED` y se indica cuál.
  - **Mientras se comprueba:** el estado es `PENDING` («Comprobando los ingredientes con IA…»).
- **Qué cambia en el producto:** el estado del análisis **no cambia** (sigue DESCONOCIDO). El resultado de la IA va en un campo aparte, `aiCheck`.
- **Probado en preproducción:**
  - 5 productos `VERIFIED`, entre ellos un zumo de naranja nuevo, que pasó de PENDING a VERIFIED en unos 40 s.
  - Un fallo de conexión en la primera petición, cuando el modelo aún se estaba cargando.
- **Arreglos tras ese fallo:**
  - Se «calienta» el modelo al arrancar.
  - Si una comprobación falla, se repite: primero al momento y luego hasta 3 intentos, con 2 y 4 minutos de espera.
  - **El camino de fallo no se ha podido provocar a propósito**, así que los reintentos no están probados en vivo.
- **Limitaciones conocidas:**
  - Solo se comprueban productos **descargados de nuevo** desde Open Food Facts.
    - Los que ya estaban en caché antes de activar la IA tendrán resultado cuando se refresquen, a las 24 h de su última descarga.
    - Entre ellos está `off-8431876310658` (garbanzos), que sigue sin resultado.
  - La cola está en memoria: si el servidor se reinicia, los productos pendientes se vuelven a pedir en el siguiente refresco.
  - La cobertura es baja a propósito. Muchas listas españolas de Open Food Facts tienen ingredientes sin reconocer, y entonces no se le pregunta a la IA.

### Cambio en el servidor
- Se han añadido dos líneas a `/opt/safeeat/infrastructure/production/preproduction.env`: un comentario y `AI_LOCAL_URL=http://ai:8080`.
- No se pudo hacer copia del fichero (permiso denegado). Para desactivar la IA basta con borrar esa línea y volver a desplegar.
- Los modelos están en `/opt/safeeat/models/`. El 0.5B no se usa y se puede borrar (unos 400 MB).
- **`production.env` no se ha tocado.**

## Funciones: qué está hecho y qué no

Leyenda:
- ✅ hecho y probado
- 🟡 hecho, sin probar del todo
- ⏳ pendiente

### Servidor (contrato de la API)

| Función | Estado | Notas |
|---|---|---|
| Registro, login, refresh, logout, completar perfil, cambiar email y contraseña, borrar cuenta | ✅ | smoke-api y la app 1.0.13 en el emulador |
| Escanear código (`/products/barcode`), producto, búsqueda por nombre con productos ya evaluados y filtro `glutenFree` | ✅ | smoke-api |
| Analizar texto de etiqueta (`analyze-text`) | ✅ | smoke-api |
| Analizar foto con IA de Claude (`analyze-photo`) | 🟡 | Requiere `ANTHROPIC_API_KEY` en el servidor (la pones tú). No se ha probado con una clave real. |
| Veredicto personal (`PUT/DELETE /products/:id/verdict`) y aviso «los ingredientes han cambiado» | ✅ | smoke-api |
| Sello certificado / declaración «sin gluten» desde las etiquetas de Open Food Facts | ✅ | tests y preproducción |
| Reseñas de productos y restaurantes, confirmar reseña e informe de gluten | ✅ | smoke-api |
| Favoritos, colecciones y guardados | ✅ | smoke-api |
| Compartir (`/p/:token`, `/r/:token`, visor web) | ✅ | smoke-api y navegador |
| Restaurantes: búsqueda sin acentos, «burger k» encuentra la cadena, locales más cercanos y valoraciones | ✅ | smoke-api |
| Cadenas: `/restaurants/chains` y `/chains/:key`. Solo las cuentas de confianza marcan una cadena; las demás reciben 403. | ✅ | smoke-api. Las sugerencias de cadena llegan solo con `includeChains=true`, para no romper la 1.0.12. |
| Panel de administración (usuarios autorizados, moderación, verificación) | ✅ | smoke-api |
| Ingredientes en español | ✅ | local y preproducción |
| «Verificado por IA» | 🟡 | El caso normal está probado; los reintentos no (ver arriba) |

### App

| Función | App Expo 1.0.13 (la actual) | App 2.0 en Rust (Tauri + Leptos) |
|---|---|---|
| Todas las pantallas actuales (pestañas, escanear, producto, restaurantes, guardados, perfil, ajustes, admin, enlaces) | ✅ funciona contra el servidor Rust | ver sección siguiente |
| Ver «Verificado por IA» e ingredientes en español con «ver original» | ❌ La 1.0.x ignora esos campos nuevos (no rompe nada) | ver sección siguiente |
| Sugerencias de cadenas primero | ❌ La 1.0.x no manda `includeChains` | ver sección siguiente |

## App 2.0 en Rust

Ver el apartado «Progreso de la app 2.0» más abajo, que se actualiza en cada paso.

## Lo que queda para ti por la mañana

1. **Probar la preproducción con el móvil.**
2. **Producción:** cuando des el visto bueno:
   - copia de seguridad de la base de datos;
   - `scripts/deploy-remote.ps1 --env production`;
   - si quieres la IA también en producción, añadir `AI_LOCAL_URL=http://ai:8080` a `production.env`.
3. **iOS:** no se puede compilar en este PC (no es un Mac). Está **sin verificar**.
4. **Firma del APK:** el certificado para instalar encima de la versión actual solo está en los secretos de GitHub (`ANDROID_KEYSTORE_BASE64`). Los APK que se compilen aquí van firmados en modo depuración y **no se instalan encima** de la 1.0.x. El APK definitivo tiene que salir del workflow `android-builder.yml`.

## Progreso de la app 2.0

- (se completa a continuación)
