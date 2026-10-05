# SafeEat 1.0.0 — publicación verificada

Fecha: 5 de octubre de 2026.

| Elemento                        | Resultado                                                        |
| ------------------------------- | ---------------------------------------------------------------- |
| Versión instalada / versionName | 1.0.0                                                            |
| Android versionCode             | 76                                                               |
| Paquete                         | com.a1var0w.safeeat                                              |
| Commit del tag v1.0.0           | 83c91e3c85c0599c8616e1361bf02fd6aba5449a                         |
| GitHub Release                  | 403444154, publicada, estable, no borrador                       |
| Visibilidad del repositorio     | Privado                                                          |
| Asset único de la release       | SafeEat-1.0.0.apk                                                |
| Tamaño                          | 120847825 bytes                                                  |
| SHA-256                         | 4d04f4620d27e35c68e1bafa1626916ef01148aea4771cb9aed23c2399c811c8 |
| Firma                           | Certificado heredado; ver limitación en android-updates.md       |

- [Release privada](https://github.com/A1VAR0W/SafeEat/releases/tag/v1.0.0).
- [Descarga pública directa](https://chat-ink.tail552c89.ts.net:10000/safeeat-downloads/releases/SafeEat-1.0.0.apk).
- [Página de instalación](https://chat-ink.tail552c89.ts.net:10000/safeeat-downloads/).
- [CI del tag completada](https://github.com/A1VAR0W/SafeEat/actions/runs/37275299027).

La APK se compiló localmente en modo release con Java 17. El job Android por tag
se omitió de forma deliberada para no reconstruir ni sustituir el binario ya
validado. La variable temporal `ANDROID_TAG_BUILDS=paused` fue eliminada tras esa
ejecución: los siguientes tags compilarán y publicarán mediante el workflow.

CI verificó 140 tests unitarios, 31 de integración con PostgreSQL y 5 del servidor
de publicación, además de TypeScript, lint, formato, builds API/web, configuración
nativa, workflows, exportaciones Android/iOS y 20/20 checks de Expo Doctor.

El digest de GitHub, el de la APK local y el del archivo servido coinciden.
La descarga pública devuelve HTTP 200, tipo APK y el tamaño exacto. El manifiesto
contiene el identificador real de release 403444154 y code76. La página se sirve
como `text/html` y contiene el enlace estable. El servicio de publicación está
activo; los secretos permanecen fuera del código y de la app.

La actualización real se probó en Android 15 mediante una build QA aislada
0.9.9/code75: descarga con progreso, validación, autorización de fuentes
desconocidas, cancelación del instalador, reintento e instalación de 1.0.0/code76.
La URL QA fue eliminada del servidor. Con el manifiesto definitivo, la APK
instalada muestra «Ya tienes la última versión» al comprobar desde Ajustes.

La APK final pasó nuevamente la navegación de restaurantes, filtros, resultados,
selección, detalle, apertura de Google Maps, tema oscuro, texto grande, búsqueda
de productos, Guardados, perfil y escáner sin cierres inesperados observados.
No se han realizado pruebas en un teléfono físico. El emulador con renderizador
por software no dibuja ciertas etiquetas del mapa; coincide con la incidencia
[MapLibre #3648](https://github.com/maplibre/maplibre-native/issues/3648), y queda
pendiente comprobarlo en hardware real. No se ha generado una nueva IPA.

Consulta [el funcionamiento, publicación y limitaciones](android-updates.md).
