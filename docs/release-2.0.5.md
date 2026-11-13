# SafeEat 2.0.5

Rama `claude/safeeat-2.0.5`. **Servidor desplegado en producción el 13-11-2026** (ver [go-no-go-2.0.5.md](go-no-go-2.0.5.md)). Publicada el mismo día en las fuentes de SideStore y en los enlaces `SafeEat-latest.*`. `releases/latest.json` (el aviso de actualización en Android) sigue en la 1.0.12: falta el release `v2.0.5` de GitHub, ver más abajo.

## Descargas de producción

Compiladas del commit `0f343c8` (mismo código de la app que `bd303d9`), apuntando a `https://chat-ink.tail552c89.ts.net:10000`.

- **Android:** `https://chat-ink.tail552c89.ts.net:10000/safeeat-downloads/SafeEat-2.0.5-production.apk`
  - 14.058.476 bytes, versionCode 2000045, run 43461878512.
  - SHA-256 `27b4e4e6f8f2b025a826fcc0d16cfc93a46139a0d6f15ffcab7f232502cfcfd1`.
  - Firmado con el certificado de siempre (`fac61745…3b9c`): se instala encima de la 1.0.12 y de la 2.0.5 de preproducción.
  - Ojo: la 2.0.5 de preproducción y la de producción comparten identificador, así que en un mismo móvil solo puede haber una de las dos.
- **iPhone:** `https://chat-ink.tail552c89.ts.net:10000/safeeat-downloads/SafeEat-2.0.5-production.ipa`
  - 7.824.011 bytes, `CFBundleVersion` 2.0.5.53, run 43461884042.
  - SHA-256 `811994494e62bd00890c50783d704f34b0f1f97e9cdbcd90264d669d26ee2449`.
  - Sin firmar, para SideStore. Las fuentes `production.json` y `safeeat.json` ya anuncian la 2.0.5, y `SafeEat-latest.ipa` es esta misma.

### Publicación (13-11-2026)

- **iPhone:** `production.json`, `safeeat.json` y `SafeEat-latest.ipa` pasan a la 2.0.5. SideStore la ofrecerá como actualización.
- **Android, descarga manual:** `SafeEat-latest.apk` → `releases/SafeEat-2.0.5.apk` (mismo APK).
- **Android, aviso dentro de la app: pendiente.** La app 1.0.12 solo acepta un `latest.json` con el `id` y la dirección de un release de GitHub `v2.0.5` publicado. Crear ese release público no estaba permitido desde aquí. Hasta entonces, `latest.json` sigue anunciando la 1.0.12 y nadie recibe el aviso.
- **Copias de lo anterior:** `*.bak-1.0.12-20261113` junto a cada fichero (`releases/latest.json`, `production.json`, `safeeat.json`, `SafeEat-latest.ipa`). El enlace `releases/latest.apk` apuntaba a `SafeEat-1.0.12.apk`.

## Descargas de preproducción

Compiladas del commit `bd303d9`, con el arreglo del lector de fotos. Sustituyen a las primeras compilaciones 2.0.5 de hoy, en las que la foto no se leía dentro de la app.

- **Android:** `https://chat-ink.tail552c89.ts.net:10000/safeeat-downloads/SafeEat-2.0.5-preproduction.apk`
  - 14.058.476 bytes, versionCode 2000044, run 43452840377.
  - SHA-256 `d92698bb8a3504f5dd3533d799d060a7e9a25fdcccd449bedc38623cc12ef2d6`.
  - Firmado con el mismo certificado que la app instalada.
  - `preproduction-android.json` la anuncia, así que una 2.0.4 de preproducción la ofrece como actualización.
- **iPhone (SideStore):** fuente `https://chat-ink.tail552c89.ts.net:10000/safeeat-downloads/preproduction.json`.
  - IPA `SafeEat-2.0.5-preproduction.ipa`: 7.824.029 bytes, `CFBundleVersion` 2.0.5.52, run 43452845920.
  - SHA-256 `b2e0ec5cfaf5323a01a5b57fefda2c0f4bb30eebde717763ee866bf6ab3b412b`.
  - La copia del manifiesto anterior está en `preproduction.json.bak-2.0.4`.
- **CI:** run 43452847727 en verde.
- **Corregido en el compilador de Android:**
  - El paso que comprueba el certificado descargaba el APK de producción (129 MB) por el Funnel. Falló dos veces por tiempo (unos 76 kB/s en ese momento).
  - Ahora compara con la huella fijada, que he comprobado contra ese mismo APK.

## Novedades

- **Ficha de restaurante:** cabecera compacta, fotos de Wikimedia Commons con su autor y licencia (nunca inventadas), y las acciones en una sola fila.
- **Mapa:** ocupa toda la pantalla, con una hoja inferior. Pide muchas menos listas al servidor; no se han subido los límites de solicitudes. Un 429 se muestra con su tiempo de espera.
- **Ingredientes:**
  - La tabla nutricional y las advertencias del envase se apartan y no cuentan como ingredientes.
  - Aditivos de chicles en español, portugués e inglés.
  - Explicación corta, con «Ver análisis completo».
  - Si en la lista se cuela otro texto, el resultado nunca es tranquilizador.
- **Foto de ingredientes, gratis:**
  - El texto se lee en el propio móvil con Tesseract, de código abierto. La foto no sale del teléfono.
  - El texto se puede corregir antes de analizarlo.
  - Si no marca que lo ha comparado con la etiqueta, el resultado nunca es favorable.
- **Enlace al fabricante:** «Buscar en <web oficial>» para las marcas conocidas. Abre un buscador externo; SafeEat no afirma nada sobre lo que se encuentre.
- **Se elimina la IA de pago:** no hay ningún endpoint ni clave de Anthropic ni de OpenAI.
- **Cuenta:**
  - Pantalla más compacta.
  - El borrado está en Cuenta → Opciones avanzadas: pide contraseña, escribir ELIMINAR y una confirmación final. El servidor comprueba las dos cosas.
- **Cuentas de confianza** (`authorized_celiac`): sus reseñas cuentan como comprobadas automáticamente, según el rol en la base de datos. Nunca se presentan como certificación.
- **Panel de administración en la app** (solo cuentas ADMIN; el servidor comprueba el rol en cada llamada):
  - Usuarios: dar y quitar la confianza.
  - Moderación de reseñas: verificar, retirar y restaurar, siempre con motivo.
  - Registro de auditoría.
- **Guardados sin conexión:**
  - Copia por cuenta en el móvil, siempre marcada como copia, con su fecha.
  - Se borra al cerrar sesión, al caducar la sesión y al borrar la cuenta.
- **Restaurantes en toda España:**
  - Una zona está cubierta cuando está dentro del límite del extracto importado (migración 018), no porque haya un local cerca.
  - Canarias se importa como una región aparte.
- **Motor de gluten 1.2.3.**

## Comprobado por mí (sin móvil físico)

- **Tests:**
  - `cargo fmt`, `clippy -D warnings` y `cargo test` en los dos workspaces.
  - `clippy` de la UI para WASM.
  - `trunk build --release`.
  - `npm run lint`, `format:check`, `typecheck` y `test`.
- **Prueba de extremo a extremo de la API** contra PostGIS desechable: 59/59.
- **Evaluación del motor:** 0/83 falsos negativos peligrosos y 0/60 falsos positivos. No demuestra una tasa inferior al 0,01 %: harían falta ≥ 29 956 positivos etiquetados.
- **Migraciones 010–018** sobre la copia de seguridad de producción del 13-11-2026, en una base aislada:
  - 10 s.
  - Los datos de usuarios, contraseñas, roles, sesiones, favoritos, marcadores, reseñas y compartidos quedan idénticos, comprobado con una huella por tabla.
- **Vuelta atrás:** la API 1.x funciona sobre el esquema migrado, y la 2.0.5 lee lo que escribe la 1.x y acepta sus tokens (`scripts/check-rollback.mjs`).
- **Copias de seguridad:** la de producción y una del esquema migrado se restauran con integridad correcta (`scripts/verify-backup-restore.sh`).
- **Restaurantes en preproducción:**
  - Importados `spain` (153 698 locales) y `canarias` (9 726) en unos 10 minutos, con descarga incluida (1,4 GB).
  - Cubiertas en el mapa: Madrid, Getafe, Barcelona, Valencia, Sevilla, Málaga, Bilbao, Zaragoza, Toledo, Molina de Aragón, Riaza, Las Palmas, Santa Cruz de Tenerife y Palma. Lisboa, no.
- **Emulador Android (APK versionCode 2000044 instalado encima de la 2.0.4):**
  - Foto de una etiqueta elegida de la galería: texto leído sin errores, editable y con el aviso.
  - Sin sesión pide iniciarla antes de analizar.
  - Los chicles 8480000651471 salen en azul, como «sin gluten» según datos de la comunidad, sin verificar.
  - Con el APK anterior (2000043) la misma foto daba «No se ha podido leer la foto». Ese fallo es el que corrige `bd303d9`.
- **La app de producción (Expo 1.0.12) contra el servidor 2.0.5 de preproducción, en el emulador** (copia con la URL de preproducción y firmada con una clave de prueba):
  - Búsqueda por nombre, ficha de producto, mapa de restaurantes, sugerencias, inicio de sesión y guardar un favorito funcionan. El favorito quedó en el servidor.
  - La cuenta de prueba se borró al terminar.
  - No probé la cámara del escáner (el emulador no tiene).
- **Arnés de la UI en el navegador:**
  - OCR real de una etiqueta: leída sin errores en unos 3 s y sin ninguna petición externa. Una foto de ruido sale marcada como ilegible y no se puede confirmar.
  - Panel de administración.
  - Guardados sin conexión y su borrado al cerrar sesión.
  - Enlace al fabricante.

## Lista de pruebas en tus móviles (pendiente: no las doy por superadas)

### Android (APK de preproducción, instalado encima de la 2.0.4)

1. Usa el APK con versionCode 2000044. Si instalaste antes otra 2.0.5 de hoy, la app ofrecerá esta como actualización. Se instala encima sin perder la sesión. «Tu cuenta» muestra SafeEat 2.0.5.
2. **Foto de ingredientes:**
   - «Hacer foto» pide permiso de cámara y abre la cámara. «Elegir de la galería» abre la galería.
   - La primera lectura tarda algo más («Preparando el lector»).
   - El texto leído aparece y se puede corregir.
3. Una foto borrosa da «Apenas se ha podido leer la foto» y no deja marcar la comprobación.
4. Analizar sin marcar la comprobación nunca da un resultado favorable. Con «Harina de trigo» sale «contiene gluten».
5. En una ficha de Hacendado, «Buscar en mercadona.es» abre el navegador del móvil, no dentro de la app.
6. **Modo avión:** Guardados muestra «Sin conexión. Es la copia guardada…» y una ficha guardada se abre. Al cerrar sesión ya no queda nada.
7. **Panel de administración** (solo con cuenta ADMIN):
   - Usuarios, Revisiones y Registro cargan.
   - Dar y quitar la confianza queda en el Registro.
   - Una cuenta normal no ve el panel.
8. **Mapa:** Valencia, Barcelona, Sevilla, Málaga, Bilbao, Zaragoza, Toledo, una zona rural y Las Palmas muestran locales, o «sin datos» solo fuera de España.
9. Chic-kles 8402001044311 da «No se ha podido confirmar…», con la tabla nutricional apartada. Kimbo 8410044005008 no sale en verde.
10. **Borrar cuenta** (con una cuenta de prueba): sin ELIMINAR o con una contraseña incorrecta no se borra. Con las dos, se borra y vuelve al inicio.

### iPhone (IPA de preproducción, por SideStore)

Instala desde la fuente de SideStore aunque ya tuvieras una 2.0.5 de hoy: la buena es la `CFBundleVersion` 2.0.5.52. Repetir los puntos 2, 5, 6, 8 y 9. En iOS, la cámara se abre desde el selector de archivos del sistema.

## Limitaciones conocidas

- **Primer uso del OCR:** el lector ocupa unos 10 MB dentro de la app y la primera lectura carga el modelo de español. En el emulador tardó menos de 20 s; no lo he medido en un móvil real.
- **OCR en iPhone:** nunca se ha ejecutado dentro de la app de iOS. Es el punto 2 de la lista del iPhone y el que más riesgo tiene. Si falla, dímelo: hay un plan B (motor y worker en un solo `blob:`).
- **OCR:** solo modelo de español. Las etiquetas en otros idiomas se leen peor; el texto siempre se puede corregir.
- **Enlace al fabricante:** solo para unas 35 marcas con web oficial conocida. Para el resto no aparece enlace.
- **iOS, texto del permiso de cámara:** ahora explica también la foto de los ingredientes.
- **La API 1.x** sigue teniendo los avisos de `npm audit` ya conocidos; es la que se usa para volver atrás.
