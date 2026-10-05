# SafeEat 1.0.12

La verificación manual de productos por una cuenta ADMIN solo se puede realizar desde «Administración → Revisiones → Productos». El servidor bloquea la ruta normal de confirmación para ADMIN; la acción del panel comprueba que quien verifica no sea el autor y deja constancia en la auditoría. Las demás cuentas conservan las confirmaciones comunitarias.

Android usa `versionCode` 88 para actualizar desde la versión 1.0.11. La IPA de iOS está publicada en la fuente de SideStore y mantiene la instalación mediante SideStore/LiveContainer.

Validación: tipos, lint, formato, 146 pruebas unitarias y 44 de integración pasaron. La API 1.0.12 está desplegada y saludable. Ambas compilaciones móviles terminaron correctamente y están publicadas. La APK firmada usa `versionCode` 88, ocupa 128765967 bytes y su SHA-256 es `818138bc9bde11ff2fcb86b06645f393a1b0c093c4ce22922f63e11018b1c118`.

La compilación Android tardó 26 minutos y 29 segundos. El script redirigía toda la salida de Gradle a un archivo y solo la mostraba si había un error, lo que daba la impresión de bloqueo. Ahora transmite la salida de Gradle a GitHub Actions y conserva el log. La publicación también muestra cuándo empieza la subida de APK y cuándo publica el manifiesto. La subida de esta versión terminó en aproximadamente un minuto.

- [APK directa](https://chat-ink.tail552c89.ts.net:10000/safeeat-downloads/SafeEat-latest.apk).
- [Fuente de SideStore](https://chat-ink.tail552c89.ts.net:10000/safeeat-downloads/production.json).

La interfaz del panel queda pendiente de comprobación en un dispositivo físico.
