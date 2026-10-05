# SafeEat 1.0.11 — administración y revisiones

Esta versión añade un panel de administración para gestionar el rol «Celíacos autorizados» y moderar revisiones de productos y restaurantes. Las revisiones revocadas dejan de contar en las fichas y en las verificaciones; se conservan con fecha, administrador y motivo para restaurarlas o auditar la decisión. Las cuentas autorizadas pueden enviar una revisión sin motivo; el resto debe explicar su aportación.

Android usa `versionCode` 87 y conserva la firma de producción para instalarse como actualización de 1.0.10. La compilación iOS se genera como IPA sin firmar, igual que en las versiones anteriores.

La migración `009_admin_moderation.sql` y el rol ADMIN ya están aplicados en producción. El despliegue de la API con los endpoints del panel se coordina con esta publicación para que la aplicación pueda usar las funciones nuevas.

Validación de la API, integración en dispositivo y resultados de publicación se anotarán aquí cuando finalicen.
