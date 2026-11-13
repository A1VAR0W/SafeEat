# SafeEat 2.0.5 (candidata, preproducción)

Rama `claude/safeeat-2.0.5`. **Producción no se ha tocado** y `releases/latest.json` no se ha publicado.

## Descargas de preproducción

- **iPhone (SideStore):** fuente `https://chat-ink.tail552c89.ts.net:10000/safeeat-downloads/preproduction.json`, que anuncia la 2.0.5.
  - IPA `SafeEat-2.0.5-preproduction.ipa`: 7.823.050 bytes, `CFBundleVersion` 2.0.5.51, run 43441182427.
  - SHA-256 `3e6926fa2dd9f7d6e75f8542eeb353cda0219796515bdab31faaf2e20fd3287b`.
  - La copia del manifiesto anterior está en `preproduction.json.bak-2.0.4`.
- **Android:** `https://chat-ink.tail552c89.ts.net:10000/safeeat-downloads/SafeEat-2.0.5-preproduction.apk`
  - 14.042.092 bytes, versionCode 2000043, run 43448179909 (commit `0af0b81`; solo cambia el flujo de CI y un texto de iOS respecto a `b93bbc2`).
  - SHA-256 `0b8f3f1c08619225141edd73861804cc69d1e0aa423ebc29fb7e37c8a5e721ff`. Firmado con el mismo certificado que la app instalada.
  - `preproduction-android.json` anuncia la 2.0.5, así que la 2.0.4 de preproducción la ofrece como actualización.
- **CI:** run 43441153696 en verde.
- **Corregido en el compilador de Android:** el paso que comprueba el certificado descargaba el APK de producción (129 MB) por el Funnel y falló dos veces por tiempo (unos 76 kB/s en ese momento). Ahora compara con la huella fijada, que he comprobado contra ese mismo APK.

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
- **Arnés de la UI en el navegador:**
  - OCR real de una etiqueta: leída sin errores en unos 3 s y sin ninguna petición externa. Una foto de ruido sale marcada como ilegible y no se puede confirmar.
  - Panel de administración.
  - Guardados sin conexión y su borrado al cerrar sesión.
  - Enlace al fabricante.

## Lista de pruebas en tus móviles (pendiente: no las doy por superadas)

### Android (APK de preproducción, instalado encima de la 2.0.4)

1. Se instala encima sin perder la sesión. «Tu cuenta» muestra SafeEat 2.0.5.
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

Repetir los puntos 2, 5, 6, 8 y 9. En iOS, la cámara se abre desde el selector de archivos del sistema.

## Limitaciones conocidas

- **Primer uso del OCR:** el lector ocupa unos 10 MB dentro de la app y la primera lectura carga el modelo de español. No lo he medido en un móvil real.
- **OCR:** solo modelo de español. Las etiquetas en otros idiomas se leen peor; el texto siempre se puede corregir.
- **Enlace al fabricante:** solo para unas 35 marcas con web oficial conocida. Para el resto no aparece enlace.
- **iOS, texto del permiso de cámara:** sigue diciendo que la cámara es para leer el código de barras. Ahora también se usa para la foto de los ingredientes. El texto ya está corregido en el código (`Info.ios.plist`), pero no entra en este IPA, sino en la próxima compilación.
- **La API 1.x** sigue teniendo los avisos de `npm audit` ya conocidos; es la que se usa para volver atrás.
