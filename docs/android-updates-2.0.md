# Actualizaciones de la app Android (SafeEat 2.x)

La 1.x se documenta en `docs/android-updates.md`. Esta es la versión en Rust (Tauri).

## Qué ve ella

1. Al abrir SafeEat, unos 2 segundos después, aparece la hoja **«SafeEat X disponible»** si hay una versión nueva, con dos botones: **Actualizar** y **Ahora no**.
2. **Actualizar** descarga la versión con una barra de progreso. Se puede cancelar.
3. **Solo la primera vez**, Android pide permiso para instalar apps desde SafeEat. La hoja lo explica:
   - pulsa «Abrir ajustes»;
   - activa «Permitir de esta fuente»;
   - vuelve a SafeEat, y la instalación sigue sola.
4. Android pregunta «¿Quieres actualizar esta aplicación?». Hay que pulsar **Actualizar**. Se conservan los datos y la sesión.
5. Para buscar a mano: **Tu cuenta → Versión de la app → Buscar actualizaciones**. El mismo botón está en la pantalla de inicio de sesión.

En iPhone no aparece nada: allí actualiza SideStore.

## Seguridad (`apps/app/plugins/updater`)

- **Manifiesto que lee cada build:**
  - preproducción: `safeeat-downloads/preproduction-android.json`;
  - producción: `safeeat-downloads/releases/latest.json`, el mismo archivo que usa la 1.x;
  - desarrollo: ninguno.
- **Origen de la descarga:** solo HTTPS desde `chat-ink.tail552c89.ts.net:10000`, y solo una de estas rutas:
  - `/safeeat-downloads/releases/SafeEat-x.y.z.apk`
  - `/safeeat-downloads/SafeEat-x.y.z-preproduction.apk`
- **Cómo se decide si hay versión nueva:** se compara el `versionCode`.
- **Comprobaciones antes de abrir el instalador:**
  - tamaño exacto;
  - SHA-256;
  - paquete `com.a1var0w.safeeat`;
  - `versionCode` anunciado y mayor que el instalado;
  - **mismo certificado de firma** que la app instalada.
- Android siempre pide confirmación: nunca se instala nada en silencio.

## Publicar una versión de preproducción

```bash
bash scripts/publish-preproduction-android.sh SafeEat-2.0.4-preproduction.apk "Notas en texto plano"
```

El script:

1. lee el paquete y las versiones del propio APK (`aapt`);
2. sube el APK y comprueba su SHA-256 en el servidor;
3. **después** escribe el manifiesto, con copia del anterior;
4. comprueba lo que verá la app.

Nunca toca `releases/latest.json`, que es el de producción.

El APK tiene que venir de `android-builder.yml`. Los APK compilados en local llevan otra firma (`a78ae558…`), y Android no permite actualizar entre firmas distintas.

## Primera instalación con actualizador

La 2.0.2 y las versiones anteriores no traen actualizador. La primera versión que lo trae hay que instalarla a mano una vez, desde la página de descargas. A partir de ahí se actualiza desde la propia app.
