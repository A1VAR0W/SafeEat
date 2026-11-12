# SafeEat 2.0: estado (noche del 12-11-2026)

Rama: `claude/safeeat-2.0-rust` (subida a GitHub, sin fusionar con `main`).
**Producción no se ha tocado.** Solo está desplegada la **preproducción** (`:10000/preproduction`).
Tampoco se ha publicado `latest.json` ni los manifiestos de SideStore.
HallowFest ha devuelto 200 antes y después de cada despliegue.

## Auditoría y correcciones (12-11-2026, segunda tanda)

Esta sección manda sobre lo que diga el resto del documento donde no coincidan.

**Importante:** desplegado en **preproducción** el 12-11-2026 (`bce9ee6`, compilado en el propio servidor con `scripts/deploy-remote.sh --build-on-server` porque Docker Desktop no arranca en este PC). Migración `013` aplicada. Comprobado en vivo con una cuenta temporal: el motor responde 1.1.0 y «Harina de trigo, azúcar, no contiene leche» da «contiene gluten». Producción no se ha tocado.

### Seguridad alimentaria

| Cambio                                                                                                                                                                                                                                                        | Estado                                       |
| ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------- |
| **Negaciones del motor de gluten.** «Harina de trigo, no contiene leche» daba «No verificado» porque cualquier «no/sin» de la frase anulaba el trigo. Ahora cada ingrediente se juzga por separado y una negación solo cuenta si va justo delante del cereal. | ✅ Implementado y probado                    |
| El motor reconoce cereales con gluten en francés, italiano, alemán, neerlandés, portugués y catalán, y el trigo sarraceno en esos idiomas.                                                                                                                    | ✅ Implementado y probado                    |
| Motor TypeScript y motor Rust idénticos: 1126 casos de referencia, 61 nuevos. Versión del motor 1.1.0.                                                                                                                                                        | ✅ Implementado y probado                    |
| Los productos en caché analizados con el motor antiguo se vuelven a pedir al proveedor.                                                                                                                                                                       | ✅ Desplegado en preproducción               |
| **Aportaciones de la comunidad.** Una etiqueta aportada por un usuario ya no sustituye los ingredientes ni el análisis del proveedor. Solo puede añadir un aviso de gluten o trazas. Nunca convierte un producto en «sin gluten».                             | ✅ Probado con tests; en preproducción       |
| Los datos originales se conservan: campos nuevos `communityIngredients` y `providerAssessment` (aditivos; la app 1.0.x los ignora). Misma regla en el servidor Rust y en la API de Node.                                                                      | ✅ Probado con tests; en preproducción       |
| **La IA ya no sale en verde ni se llama «verificado».** La app muestra «Análisis orientativo por IA» en un tono azul neutro, con el aviso siempre visible. Las reseñas de confianza de la comunidad tampoco salen en verde.                                   | ✅ Probado con tests de la app               |
| El verde queda solo para una declaración «sin gluten» de una fuente identificada o para el veredicto propio de la usuaria. El aviso del sello dice de dónde sale y que SafeEat no puede comprobar el certificado.                                             | ✅ Probado con tests de la app               |
| Las comprobaciones de IA pendientes se guardan en la base de datos (migración aditiva `013`) y se recuperan tras un reinicio.                                                                                                                                 | 🟡 Compila y migra en CI; no probado en vivo |
| Reintentos de la IA tras un fallo.                                                                                                                                                                                                                            | 🟡 Sigue sin poder provocarse el fallo       |

Casos que cambian de resultado con el motor 1.1.0 (de 678 casos antiguos, 7): «no contiene trigo ni cebada» pasa de «contiene gluten» a «no verificado», y el campo de trazas con «sin trigo» pasa a «puede contener trazas». Ninguno pasa a «sin gluten».

**No existe la categoría «certificación comprobada».** SafeEat no tiene forma de verificar un certificado contra el registro del certificador; solo sabe que Open Food Facts indica el sello. Por eso se muestra como declaración con su fuente.

### CI (GitHub Actions)

La ejecución del commit `52b5c1e` y las de los commits posteriores hasta `1088338` pasaron enteras, con tres trabajos:

- **verify** (TypeScript/Expo): lint, formato, tipos, 208 tests, tests de integración con PostgreSQL, compilación y comprobación de Expo.
- **rust**: `cargo fmt`, `clippy` con avisos como errores, tests, migraciones sobre una base con datos de la 1.0.x y luego la batería `smoke-api` (41 comprobaciones) contra esa base aislada.
- **app**: `cargo fmt`, `clippy`, tests de la interfaz, compilación a WebAssembly y compilación del núcleo Tauri.

Fallos reales que tapaba el CI anterior y se han arreglado:

- ESLint y Prettier analizaban la salida de compilación de Rust y el MapLibre de terceros.
- Los tests de integración de Node buscaban las migraciones en la carpeta antigua.
- Dos tests del importador antiguo esperaban las reglas de cadenas de la 1.0; ahora siguen las de la 2.0.
- Tres paquetes de Expo iban un parche por detrás.

### Sesión

- La sesión ya **no se pierde al cerrar la app**. El token de refresco y el perfil se guardan cifrados con una clave del Android Keystore (en iOS, Keychain). Plugin propio `tauri-plugin-safeeat-secure`, sin ningún comando accesible desde JavaScript.
- Probado en el emulador: iniciar sesión, cerrar la app a la fuerza y reabrir → sigue dentro. En disco solo hay texto cifrado. Cerrar sesión deja el almacén vacío. Una sesión guardada de una cuenta ya borrada se rechaza y se olvida sola.
- Probado también: siete reinicios seguidos (el token rota en cada uno), arrancar en modo avión (sigue con la sesión) y volver a tener red (las llamadas autenticadas funcionan).
- **iOS: código escrito, sin compilar ni probar.**
- Sin probar: caducidad del token de acceso a los 10 minutos con la app abierta.

### Funciones nuevas de la app 2.0

- Ajustes de cuenta: completar perfil, cambiar correo, cambiar contraseña y borrar cuenta. **Borrar cuenta está probado en el emulador**; cambiar correo y contraseña y completar perfil compilan pero no se han probado en pantalla.
- **Revisiones de la comunidad** en productos y restaurantes: lista de revisiones y botón «Yo también lo he comprobado», con un segundo paso de confirmación. No aparece para tus propias revisiones ni para las de cuentas de confianza. Nada se pinta de verde ni se llama «verificado». Probado en el emulador contra preproducción con dos cuentas temporales.
- **«¿Qué dice la etiqueta?»**: revisión de un producto con código de barras (sin gluten / contiene gluten y qué has comprobado). Probado en el emulador.
- **Valoración con estrellas** de restaurantes: guardar, cambiar y quitar. Probado en el emulador.
- **Compartir** productos y restaurantes: el núcleo Tauri crea el enlace público y abre el menú de compartir del sistema con un plugin nativo nuevo (`tauri-plugin-safeeat-share`: selector de Android, hoja de compartir en iOS). En Android probado en el emulador; **el código de iOS está escrito pero sin compilar**.
- **Guardados** con dos pestañas, Productos y Restaurantes. En Productos: «Favoritos», «Me sientan bien» y «No son seguros» (su veredicto personal, «Mis productos seguros») y sus marcadores. Se pueden crear y eliminar marcadores, y desde la ficha de un producto, «Organizar en marcadores». Los restaurantes se guardan con el corazón de su ficha. Probado en el emulador.
- **Historial en el móvil**: productos vistos hace poco y búsquedas recientes, con «Borrar historial». Solo guarda nombre, marca e imagen, nunca el estado de gluten (al abrirlo se ve el análisis actual), ni el veredicto personal, ni los productos creados desde una etiqueta propia. Probado en el emulador.
- Todas las cuentas de prueba se borraron después; sus revisiones y valoraciones desaparecieron con ellas.

### Restaurantes

- En la app 2.0 la información favorable de un restaurante («opciones sin gluten», «todo sin gluten») se muestra en azul orientativo y no en verde: viene de otros usuarios o de OpenStreetMap, y una sola reseña bastaba para pintarlo de verde. Los avisos conservan su color. El servidor no cambia, así que la app 1.0.x lo sigue mostrando como antes.
- No hay datos verificables sobre freidoras exclusivas ni protocolos de cocina; no se ha añadido nada que lo aparente.
- **Catálogo de preproducción solo con Madrid:** tiene 17 375 locales, todos de la zona de Madrid. Fuera de ahí el servidor consulta Overpass, que el 12-11-2026 tardaba más de 20 s y devolvía «La fuente de información no está disponible» (por ejemplo, en Valencia). No es un fallo del código nuevo. Para probar en otra ciudad hay que importar su catálogo en preproducción.

### Infraestructura y claves

- **Claves:** se ha buscado en el árbol y en todo el historial de Git (claves privadas, claves de Anthropic, tokens de GitHub, ficheros `.env`, keystores): no aparece ninguna.
- **Compilaciones de publicación:** una compilación de publicación de la app sin `SAFEEAT_API_URL` ya no compila. Antes usaba preproducción sin avisar.
- **Sin auditar en esta tanda:** copias de seguridad cifradas externas y su restauración, estado de salud de los contenedores, contenido de los logs y consumo real de recursos en el servidor. Los límites de memoria y CPU del contenedor de IA ya estaban puestos.
- El workflow `android-builder.yml` sigue siendo el de la app Expo; no compila todavía la app 2.0.

### Efecto en la app Expo 1.0.x

Cuando se despliegue el servidor nuevo, la etiqueta que aporte un usuario dejará de sustituir los ingredientes del proveedor. La 1.0.x no muestra el campo nuevo `communityIngredients`, así que ahí la aportación solo se verá si el proveedor no tenía ingredientes o si añade un aviso.

### Sin ver en pantalla

- La tarjeta «Análisis orientativo por IA» y la etiqueta de la comunidad solo están cubiertas por tests; no se encontró en preproducción un producto con resultado de IA para verla.
- Cambiar correo, cambiar contraseña y completar perfil.

### Bloqueado

- ~~Desplegar en preproducción.~~ Hecho el 12-11-2026 compilando en el servidor (`--build-on-server`). Docker Desktop en este PC sigue sin arrancar.

### Pendiente (no implementado)

OCR de etiquetas, análisis de fotos con Claude, añadir restaurante, caché sin conexión, selector de 1, 2 o 3 columnas en Guardados, panel de administración, enlaces profundos, ajustes de apariencia, escaneo continuo, actualizador del APK.

### Builds de preproducción publicados (12-11-2026)

- **Android:** `android-builder.yml` con `app=rust` (run 43310502312). APK arm64 de release, versionCode 2000036, firmado con la clave de la 1.x. El workflow compara su certificado con el de `SafeEat-latest.apk` y comprueba los permisos, que no sea depurable y la URL de la API. Publicado en `safeeat-downloads/SafeEat-2.0.0-preproduction.apk`. **No** está en `latest.json`.
- **iOS:** `ios-builder.yml` con `app=rust` (run 43304761298). IPA sin firmar, publicada en SideStore con el manifiesto `safeeat-downloads/preproduction.json` del servidor. Se ha hecho una copia de seguridad del manifiesto 0.2.0. La copia de `main` en GitHub sigue en 0.2.0.
- **Ninguno está probado todavía en un dispositivo físico.** La 2.0 no tiene actualizador: cada versión de PRE se instala a mano. Al ser versionCode 2000036, volver a la 1.x en Android exige desinstalar.

La app 2.0 **todavía no alcanza a la app Expo**: no se debe retirar la 1.0.x.

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

### Segunda opinión de la IA (antes «Verificado por IA»; ya no se muestra en verde)

- **Modelo:** Qwen2.5 1.5B (Q4_K_M) con llama.cpp, en un contenedor `ai` del propio servidor.
- **Límites del contenedor:**
  - Memoria: 1,4 GB como máximo (en reposo usa unos 170 MB).
  - CPU: 1,5 núcleos.
  - Sin acceso a internet, en solo lectura y sin privilegios.
- **Por qué es tan pequeño:** el servidor tiene un AMD E2-6110 sin AVX2 y 6,7 GB de RAM.
  - Lee unos 6 tokens/s y genera unos 2,5 tokens/s.
  - Un modelo mayor dejaría sin recursos a HallowFest.
- **Lo que se midió con 50 listas reales** (30 con gluten y 20 sin gluten), contando cuántas veces dijo «sin gluten» a una lista que sí lo tenía:

| Cómo se le preguntó                     | Errores en las 30 listas con gluten |
| --------------------------------------- | ----------------------------------- |
| Lista completa, modelo 0.5B             | 22                                  |
| Lista completa, modelo 1.5B             | 28                                  |
| Ingrediente a ingrediente               | 2 (seitán, escanda)                 |
| Ingrediente a ingrediente, con ejemplos | 1 (bulgur)                          |

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

| Función                                                                                                                  | Estado | Notas                                                                                                |
| ------------------------------------------------------------------------------------------------------------------------ | ------ | ---------------------------------------------------------------------------------------------------- |
| Registro, login, refresh, logout, completar perfil, cambiar email y contraseña, borrar cuenta                            | ✅     | smoke-api y la app 1.0.13 en el emulador                                                             |
| Escanear código (`/products/barcode`), producto, búsqueda por nombre con productos ya evaluados y filtro `glutenFree`    | ✅     | smoke-api                                                                                            |
| Analizar texto de etiqueta (`analyze-text`)                                                                              | ✅     | smoke-api                                                                                            |
| Analizar foto con IA de Claude (`analyze-photo`)                                                                         | 🟡     | Requiere `ANTHROPIC_API_KEY` en el servidor (la pones tú). No se ha probado con una clave real.      |
| Veredicto personal (`PUT/DELETE /products/:id/verdict`) y aviso «los ingredientes han cambiado»                          | ✅     | smoke-api                                                                                            |
| Sello certificado / declaración «sin gluten» desde las etiquetas de Open Food Facts                                      | ✅     | tests y preproducción                                                                                |
| Reseñas de productos y restaurantes, confirmar reseña e informe de gluten                                                | ✅     | smoke-api                                                                                            |
| Favoritos, colecciones y guardados                                                                                       | ✅     | smoke-api                                                                                            |
| Compartir (`/p/:token`, `/r/:token`, visor web)                                                                          | ✅     | smoke-api y navegador                                                                                |
| Restaurantes: búsqueda sin acentos, «burger k» encuentra la cadena, locales más cercanos y valoraciones                  | ✅     | smoke-api                                                                                            |
| Cadenas: `/restaurants/chains` y `/chains/:key`. Solo las cuentas de confianza marcan una cadena; las demás reciben 403. | ✅     | smoke-api. Las sugerencias de cadena llegan solo con `includeChains=true`, para no romper la 1.0.12. |
| Panel de administración (usuarios autorizados, moderación, verificación)                                                 | ✅     | smoke-api                                                                                            |
| Ingredientes en español                                                                                                  | ✅     | local y preproducción                                                                                |
| «Verificado por IA»                                                                                                      | 🟡     | El caso normal está probado; los reintentos no (ver arriba)                                          |

### App

| Función                                                                                                               | App Expo 1.0.13 (la actual)                           | App 2.0 en Rust (Tauri + Leptos) |
| --------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------- | -------------------------------- |
| Todas las pantallas actuales (pestañas, escanear, producto, restaurantes, guardados, perfil, ajustes, admin, enlaces) | ✅ funciona contra el servidor Rust                   | ver sección siguiente            |
| Ver «Verificado por IA» e ingredientes en español con «ver original»                                                  | ❌ La 1.0.x ignora esos campos nuevos (no rompe nada) | ver sección siguiente            |
| Sugerencias de cadenas primero                                                                                        | ❌ La 1.0.x no manda `includeChains`                  | ver sección siguiente            |

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

Código en `apps/app`. Es un workspace propio: `src-tauri` es el núcleo y `ui` la interfaz en Leptos. Se ha diseñado con la skill ui-ux-pro-max:

- colores de marca;
- fuente del sistema, sin descargas;
- botones de al menos 44–48 px;
- cada estado con icono y texto, no solo color;
- modo oscuro;
- movimiento corto y respeto de «reducir movimiento».

**Rendimiento:**

- La interfaz pesa unos 600 KB de WebAssembly y 43 KB de JS.
- MapLibre (940 KB) solo se carga al abrir Restaurantes, y el mapa no se destruye al cambiar de pestaña.
- Todas las llamadas a la API pasan por Rust (reqwest). Los tokens nunca llegan al JavaScript.

Leyenda de esta tabla:

- ✅ probado en el emulador contra la preproducción
- 🟡 hecho, probado solo en parte
- ⏳ pendiente

| Pantalla / función                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 | Estado | Qué se probó                                                                                                                                                                                    |
| -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Escribir el código a mano                                                                                                                                                                                                                                                                                                                                                                                                                                                                          | ✅     | Abre la ficha del producto                                                                                                                                                                      |
| Escanear con la cámara (plugin oficial, en ventana, con botón «Cancelar»)                                                                                                                                                                                                                                                                                                                                                                                                                          | 🟡     | La cámara se abre y «Cancelar» y el botón Atrás la cierran bien. **No se ha probado que lea un código real** (la cámara del emulador muestra una escena virtual). Hay que probarlo en el móvil. |
| Ficha de producto, en este orden: respuesta en una línea → tu veredicto → sello → análisis → «Verificado por IA» → ingredientes → comunidad                                                                                                                                                                                                                                                                                                                                                        | ✅     |                                                                                                                                                                                                 |
| «Verificado por IA» en verde con el aviso «La IA puede equivocarse: revisa la etiqueta» siempre visible                                                                                                                                                                                                                                                                                                                                                                                            | ✅     |                                                                                                                                                                                                 |
| «La IA ve posible gluten» y «Comprobando con IA…» (se refresca solo)                                                                                                                                                                                                                                                                                                                                                                                                                               | 🟡     | Están en el código y en los tests, pero no se ha encontrado ningún producto en ese estado para verlo                                                                                            |
| Ingredientes en español con «Ver original» y alérgenos en negrita                                                                                                                                                                                                                                                                                                                                                                                                                                  | ✅     | Producto francés `3017760000109`                                                                                                                                                                |
| Veredicto personal (me sienta bien / no es seguro)                                                                                                                                                                                                                                                                                                                                                                                                                                                 | ✅     | Aparece «Marcado por ti como seguro»                                                                                                                                                            |
| Aviso «los ingredientes han cambiado»                                                                                                                                                                                                                                                                                                                                                                                                                                                              | 🟡     | Probado en el servidor (smoke-api), no en la app                                                                                                                                                |
| Favoritos (corazón) y pestaña Guardados                                                                                                                                                                                                                                                                                                                                                                                                                                                            | ✅     |                                                                                                                                                                                                 |
| Búsqueda de productos por nombre, con cada resultado ya evaluado, y filtro «Solo sin gluten declarado»                                                                                                                                                                                                                                                                                                                                                                                             | ✅     | «galletas»: con el filtro quedan 2, igual que la API                                                                                                                                            |
| Mapa (MapLibre + OpenFreeMap): agrupación de marcadores, tu ubicación, lista de resultados                                                                                                                                                                                                                                                                                                                                                                                                         | ✅     |                                                                                                                                                                                                 |
| Filtros del mapa («Con opciones sin gluten», «Todo sin gluten») y «Buscar en esta zona» al mover el mapa                                                                                                                                                                                                                                                                                                                                                                                           | ✅     |                                                                                                                                                                                                 |
| Sugerencias con las cadenas primero                                                                                                                                                                                                                                                                                                                                                                                                                                                                | ✅     | «burger k» → «Cadena Burger King · 184 locales»                                                                                                                                                 |
| Pantalla de cadena con los locales más cercanos                                                                                                                                                                                                                                                                                                                                                                                                                                                    | ✅     | Ordenados por distancia (130 m, 140 m, 410 m…)                                                                                                                                                  |
| Ficha de restaurante: cómo llegar, llamar, cadena, en qué se basa                                                                                                                                                                                                                                                                                                                                                                                                                                  | ✅     |                                                                                                                                                                                                 |
| Reseña de restaurante con un comentario de al menos 10 caracteres                                                                                                                                                                                                                                                                                                                                                                                                                                  | ✅     | Con una cuenta normal: solo se ofrece «este local», como debe ser                                                                                                                               |
| Reseña de toda la cadena (cuentas de confianza)                                                                                                                                                                                                                                                                                                                                                                                                                                                    | 🟡     | El selector solo aparece a cuentas de confianza; no hay ninguna de prueba en preproducción. El 403 para las demás está probado en smoke-api.                                                    |
| Login                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              | ✅     |                                                                                                                                                                                                 |
| Registro                                                                                                                                                                                                                                                                                                                                                                                                                                                                                           | 🟡     | Hecho; la cuenta de prueba se creó por la API, no desde la pantalla                                                                                                                             |
| **La sesión dura mientras la app está abierta.** El almacenamiento seguro (Keystore/Keychain) es un plugin propio que falta. No se guarda nada en texto plano.                                                                                                                                                                                                                                                                                                                                     | ⏳     |                                                                                                                                                                                                 |
| **No implementado todavía en la 2.0** (la app 1.0.13 sí lo tiene casi todo): OCR de etiquetas, IA sobre una foto, reseñar o confirmar la reseña de un producto, compartir, valoración con estrellas, añadir un restaurante, colecciones, «Mis productos seguros» y selector de columnas, historial de búsquedas, caché sin conexión, completar perfil y ajustes de cuenta (email, contraseña, borrar cuenta), panel de administración, enlaces `/p/` `/r/`, escaneo continuo, actualizador del APK | ⏳     |                                                                                                                                                                                                 |
| Compilación para móviles reales (aarch64), APK firmado de publicación, iOS                                                                                                                                                                                                                                                                                                                                                                                                                         | ⏳     | Solo se ha compilado el APK de depuración x86_64 para el emulador (~172 MB por ser de depuración)                                                                                               |

**Fallos encontrados en las pruebas y ya arreglados:**

- El mapa salía en blanco: el CSS de MapLibre pisaba el tamaño.
- No pedía la ubicación: faltaban permisos del plugin.
- El escáner a pantalla completa no tenía salida: ahora va en ventana con «Cancelar», y el botón Atrás también lo cierra.
- La pantalla de cadena se quedaba en «Cargando…» por un error interno.
- La reseña de restaurante fallaba siempre porque el servidor pide un comentario y la app no tenía dónde escribirlo.
- La sugerencia de cadena repetía el texto («Cadena Cadena…»).
- La app se colgaba si cambiabas de pestaña mientras esperaba una respuesta (por ejemplo, una búsqueda). Ahora ignora la respuesta si la pantalla ya no está.

**Datos de prueba:** la cuenta de prueba usada en el emulador se borró al terminar, y con ella su reseña, su favorito y su veredicto. El Burger King de la calle Carretas ha vuelto a «No verificado».

**Sobre el emulador:** para instalar la 2.0 se desinstaló la app Expo del emulador (no del móvil), porque el APK de depuración va firmado con otra clave.

**Nota sobre Git:** el commit `2063029` subió por error la carpeta de compilación `apps/app/target`. Se quitó en el commit siguiente, pero sigue en el historial de esta rama. **Al fusionar con `main`, usa «Squash and merge»** para que no entre.
