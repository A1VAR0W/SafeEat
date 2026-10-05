# SafeEat 1.0.2 — firma de las actualizaciones

La APK original 1.0.0 reproduce en Android 10 / API 29 el error «La firma de la
actualización no coincide con SafeEat» al descargar la APK oficial 1.0.1, aunque
ambas tienen exactamente el mismo certificado.

El origen es la llamada a `getPackageArchiveInfo` con solo
`GET_SIGNING_CERTIFICATES`. La implementación de
[Android 10 en AOSP](https://raw.githubusercontent.com/aosp-mirror/platform_frameworks_base/android-10.0.0_r1/core/java/android/content/pm/PackageManager.java)
solo ejecuta la lectura de certificados si también está presente `GET_SIGNATURES`.
Solicitamos ambos indicadores en API 28+, manteniendo la comparación exacta de
los certificados actuales de `signingInfo`. No se acepta una firma vacía ni se
recurre a certificados históricos cuando existe una discrepancia. El mensaje
distingue ahora la ausencia de información de una firma realmente diferente.

## Validación

- APK original 1.0.0 en emulador Android 10: error reproducido y capturado.
- Build QA local 1.0.0 con únicamente el arreglo nativo: descarga de la misma
  APK oficial 1.0.1, validación correcta, permiso de fuentes desconocidas,
  apertura del instalador y actualización completada sin desinstalar.
- La build QA no se publica y no modifica el manifiesto de producción.
- APK final 1.0.2 instalada encima de 1.0.1 en Android 10: instalación correcta
  y versión 1.0.2 visible en Ajustes.
- 29 tests de configuración/versionado/actualizador, TypeScript móvil,
  compilación Android release y verificación apksigner/aapt correctos.

Versión final: 1.0.2, `versionCode` 78. El archivo es `SafeEat-1.0.2.apk` y su
SHA-256 es `ac46d363ddef62480f7d6b51d76bf1a1ed4b321e60672ef55246b868323906c0`.
Se conserva el certificado de las versiones anteriores. La firma no se desactiva.

El usuario afectado debe instalar la APK 1.0.2 directamente encima de la anterior:
la lógica defectuosa de 1.0.0/1.0.1 no puede corregirse desde el servidor. Después
de esa instalación podrá usar el actualizador corregido. No se requiere borrar
datos ni desinstalar SafeEat.

El usuario confirmó descarga oficial y dispositivos Xiaomi/Samsung, sin versión
de Android concreta. La validación se hizo en emulador Android 10; no en esos
teléfonos físicos. La comprobación previa en Android 15 no cubría este fallo.
