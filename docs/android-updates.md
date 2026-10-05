# Actualizaciones Android de SafeEat

La aplicación real utiliza Expo 55, React Native 0.83.10 y React 19.2. No utiliza
Ionic ni Capacitor. El actualizador es un módulo local de Expo escrito en Kotlin;
se registra únicamente en Android. No se ha añadido ningún paquete externo.

## Distribución privada y flujo del usuario

El repositorio `A1VAR0W/SafeEat` sigue siendo privado. GitHub Releases es la fuente
de verdad: CI obtiene la release y su APK, compara el SHA-256 del archivo con el
digest del asset de GitHub y publica una copia en el servidor existente. La app
consulta ese manifiesto público, sin credenciales de GitHub.

- Descarga inicial: <https://chat-ink.tail552c89.ts.net:10000/safeeat-downloads/SafeEat-latest.apk>
- Página sencilla: <https://chat-ink.tail552c89.ts.net:10000/safeeat-downloads/>
- Manifiesto: <https://chat-ink.tail552c89.ts.net:10000/safeeat-downloads/releases/latest.json>

Desde 1.0.0, al abrir la app se inicia una comprobación diferida 1,8 segundos.
No bloquea la navegación. Se deduplican peticiones concurrentes y se limita la
comprobación automática a una por hora en la misma sesión. Ajustes permite
comprobar manualmente. La versión se compara numéricamente por componentes:
`1.0.10` es posterior a `1.0.9`; también debe aumentar `versionCode`.

Una versión superior muestra un modal con notas, Actualizar y Ahora no. La
descarga tiene progreso y cancelación. Después de verificar el archivo, SafeEat
abre el instalador de Android. Si falta permiso, explica cómo habilitar la
instalación desde SafeEat y continúa al regresar. Cancelar el instalador permite
reabrirlo sin descargar otra vez. Una interrupción permite reintentar.

La APK anterior a 1.0.0 necesita esta primera instalación manual. No es necesario
desinstalarla. Las siguientes actualizaciones se ofrecen dentro de SafeEat.

Los errores de comprobación se registran y no muestran avisos automáticos ni
bloquean la app. La comprobación tiene un tiempo máximo de 8 segundos. Un fallo
del servidor deja disponible la última publicación correcta; no se publica un
manifiesto antes de comprobar la APK.

## Archivos

Nuevos:

- `apps/mobile/src/updates/`: modelo validado, comparación semántica, servicio,
  puente nativo, modal y tests.
- `apps/mobile/modules/safeeat-updater/`: módulo Android, manifest y FileProvider.
- `release.json`: contador Android.
- `scripts/set-version.mjs`: incremento centralizado de versión y contador.
- `scripts/publish-android-release.mjs`: verifica GitHub y publica en el servidor.
- `infrastructure/production/release-publisher.py`, su servicio systemd y tests:
  recepción autenticada y publicación atómica.
- `apps/mobile/src/restaurant-{map,components,presentation}.tsx/ts` y pruebas:
  mapa, selección, estados y direcciones compartidas.

Modificados: configuración Expo, layout raíz, Ajustes, pantalla de restaurantes,
ficha de restaurante, `package.json`, lockfile, workflows Android y CI,
`build-android.sh`, nginx, página de descarga y `.gitignore`.

## Android y seguridad

El módulo descarga mediante HTTPS en un hilo de trabajo. Solo admite el origen
y la ruta de releases configurados, sin redirecciones ni URLs arbitrarias.
Comprueba tamaño, SHA-256, nombre de paquete, versión, contador y el certificado
de firma contra la app instalada. Antes de abrir el instalador vuelve a validar
el archivo. Los temporales se guardan en caché privada y se limpian al reiniciar
el módulo. No se solicita acceso general al almacenamiento para actualizar.

El FileProvider no está exportado y solo comparte el directorio de actualizaciones,
con permiso temporal de lectura. `REQUEST_INSTALL_PACKAGES` permite pedir al
usuario la autorización del sistema; no permite instalaciones silenciosas.
Compatibilidad mínima de la APK: Android 7 / API 24; destino API 36.

Se conserva el certificado de las APK anteriores para permitir actualizar encima.
Es el certificado heredado de pruebas (`CN=Android Debug`), aunque la APK se
compila en modo release y no es depurable. **No es una clave privada definitiva
de producción.** Su SHA-256 es
`fac61745dc0903786fb9ede62a962b399f7348f0bb6f899b8332667591033b9c`.
Cambiarlo sin una migración compatible impediría actualizar las instalaciones
existentes. Antes de una distribución amplia debe planificarse esa migración.

## GitHub Actions y secretos

Se reutiliza `.github/workflows/android-builder.yml`. Al recibir `vX.Y.Z` valida
que el tag coincida con `package.json`, ejecuta TypeScript y tests, hace Expo
prebuild, compila, firma y verifica una APK universal. Crea la release estable,
adjunta exclusivamente `SafeEat-X.Y.Z.apk` y publica el espejo en el servidor.
Las builds por tag no generan ZIPs de artifacts adicionales. Se mantiene el
workflow manual para pruebas.

Secrets del repositorio ya configurados durante esta implementación:

- `ANDROID_KEYSTORE_BASE64`
- `ANDROID_KEYSTORE_PASSWORD`
- `ANDROID_KEY_ALIAS`
- `ANDROID_KEY_PASSWORD`
- `SAFEEAT_RELEASE_PUBLISH_TOKEN`

`GH_TOKEN` procede automáticamente de `github.token`. No se guarda en el móvil
ni en el servidor. El entorno GitHub `production` contiene `API_PUBLIC_URL` y
`PUBLIC_APP_URL`. Ambos apuntan al origen HTTPS de SafeEat. No es necesario crear
una cuenta nueva para usar este actualizador.

El token de publicación también reside en `/etc/safeeat-release-publisher.env`
con permisos 600. El servicio escucha exclusivamente en `127.0.0.1:8099`; nginx
publica `/safeeat-release-publish/` por HTTPS. Las cargas sin token reciben 401.
Las APK son inmutables por versión; no se acepta sustituirlas por bytes distintos
ni publicar un contador inferior. El servidor solo expone binarios y metadatos,
no el código privado. La APK pública sí puede ser descargada por quien tenga el
enlace, igual que la distribución anterior.

`ANDROID_TAG_BUILDS=paused` permite omitir temporalmente el job por tag para una
publicación local controlada. No debe quedar activado al publicar versiones nuevas.

## Publicar la siguiente versión

Con los cambios terminados, en la raíz del repositorio:

```sh
node scripts/set-version.mjs 1.0.1
npm run typecheck
npm run lint
npm run format:check
npm test
git add <archivos revisados>
git commit -m "Release SafeEat 1.0.1"
git push origin HEAD
git tag v1.0.1
git push origin v1.0.1
```

El script cambia la versión raíz y el lockfile y aumenta el contador Android de
76 a 77. Expo deriva `versionName`, la versión visible y la versión de iOS del
`package.json` raíz. `release.json` alimenta `android.versionCode`; nunca se usa
el número de ejecución de Actions para decidirlo. Los paquetes internos conservan
su versionado de librería, que no representa la versión instalada.

Revisar el resultado del workflow y comprobar `latest.json`: debe indicar el tag,
el contador y el hash nuevos. La URL `SafeEat-latest.apk` siempre apunta a la
versión publicada, y la página no necesita editarse en cada actualización.
No borrar ni reutilizar tags publicados. Si falla el espejo después de crear la
release, recuperar la APK exacta de esa release y ejecutar el publicador con el
secreto disponible en el entorno, sin imprimirlo:

```sh
gh release download v1.0.1 --pattern SafeEat-1.0.1.apk --dir artifacts
node scripts/publish-android-release.mjs v1.0.1 artifacts/SafeEat-1.0.1.apk
```

La ejecución local de 1.0.0 usa exactamente el mismo publicador que CI. Los
minutos y permisos de GitHub Actions siguen siendo necesarios para futuras
compilaciones automáticas; esta primera APK se ha compilado localmente.

## Actualización obligatoria

Las actualizaciones son opcionales por defecto. Para establecer una versión mínima,
añadir a las notas de la release, antes de ejecutar el publicador:

```html
<!-- safeeat-min-version: 1.0.1 -->
```

El publicador convierte el comentario en `minimumVersion`, y el modal oculta
Ahora no solo para instalaciones anteriores a ese mínimo. El comentario no se
muestra en las notas de la app. Editar las notas en GitHub después de publicar
requiere volver a ejecutar el publicador para actualizar el manifiesto. El mínimo
no debe superar la versión publicada. Un error de red nunca impone un bloqueo.

## Pruebas y límites de la validación

Se han ejecutado 140 pruebas de TypeScript/Vitest y 5 del puente de publicación,
TypeScript de todos los workspaces, ESLint, Prettier y validación de workflows.
La APK 1.0.0 se ha compilado con Java 17 y verificado mediante apksigner/aapt.
Expo ha exportado el bundle de iOS sin ejecutar el módulo Android.

En un emulador Android 15 / API 35 se ha instalado una build de prueba aislada
0.9.9/code75 y actualizado mediante el propio modal a la APK real 1.0.0/code76:
aviso, Ahora no, comprobación manual, progreso, verificación, permiso de fuentes
desconocidas, cancelación del instalador, reintento e instalación completada.
El manifiesto de prueba utilizaba una ruta temporal, separada de `latest.json`;
no representa una release pública. La APK final usa exclusivamente `latest.json`.

Los tests cubren 1.0.0 frente a 1.0.0 y 1.0.1, 1.0.9 frente a 1.0.10, ausencia de
red/release/APK, manifiestos inválidos, borradores/prereleases, URLs ajenas y la
exclusión de iOS/web. La interrupción de descarga elimina el parcial y permite
reintento por diseño; la prueba de instalación real no simuló todas las formas
de pérdida de red, almacenamiento agotado o cierre forzado del proceso.

Para comprobar específicamente 1.0.0 → 1.0.1: instalar esta APK, publicar una
1.0.1 real con el procedimiento anterior, abrir SafeEat o Buscar actualizaciones,
confirmar permiso e instalación y verificar 1.0.1 en Ajustes. No se ha publicado
una 1.0.1 ficticia para probarlo.

Restaurantes usa un mapa que ocupa el espacio disponible, tarjeta del lugar
seleccionado y resultados en una pantalla completa con lista virtualizada. Android
usa MapLibre/OpenFreeMap; iPhone conserva Apple Maps. Cómo llegar abre una URL de
Google Maps compatible con ambas plataformas. Se han probado filtros, selección,
ficha, apertura real de Google Maps, tema oscuro, texto grande y navegación por
Explorar, Guardados, perfil y escáner sin fallos de ejecución observados.

La precisión GPS depende del permiso de ubicación precisa, la señal y el móvil;
no se puede garantizar una posición exacta por software. La búsqueda y los filtros
sin gluten siguen dependiendo de la información del catálogo existente. No se
garantiza cobertura de todos los restaurantes ni ausencia de contaminación cruzada.

No se ha probado en un teléfono físico ni compilado una IPA en este entorno Windows.
La exportación de iOS no sustituye una compilación con Xcode ni una prueba en iPhone.
El renderizador por software del emulador ha mostrado una limitación de etiquetas
del mapa coincidente con [MapLibre #3648](https://github.com/maplibre/maplibre-native/issues/3648).
No se debe extrapolar el rendimiento del emulador al teléfono de la usuaria.

Referencias: [módulos Expo](https://docs.expo.dev/modules/get-started/),
[FileProvider](https://developer.android.com/reference/androidx/core/content/FileProvider),
[GitHub Releases](https://docs.github.com/en/rest/releases/releases),
[Google Maps URLs](https://developers.google.com/maps/documentation/urls/get-started),
[OpenFreeMap](https://openfreemap.org/quick_start/).
