# SafeEat 1.0.11 — administración y revisiones

Esta versión añade un panel de administración para gestionar el rol «Celíacos autorizados» y moderar revisiones de productos y restaurantes. Las revisiones revocadas dejan de contar en las fichas y en las verificaciones; se conservan con fecha, administrador y motivo para restaurarlas o auditar la decisión. Las cuentas autorizadas pueden enviar una revisión sin motivo; el resto debe explicar su aportación.

La IPA iOS se compiló sin firmar y se publicó en SideStore/LiveContainer. La build Android `versionCode` 87 se canceló antes de generar el APK; la próxima versión Android será 1.0.12 (`versionCode` 88) e instalará como actualización de 1.0.10.

La migración `009_admin_moderation.sql`, el rol ADMIN y los endpoints iniciales del panel ya están desplegados en producción. La verificación manual de productos por administradores desde el panel y el bloqueo de la ficha normal corresponden a 1.0.12.

La IPA contiene SafeEat 1.0.11, tamaño 19 226 975 bytes. La API responde correctamente y la fuente de SideStore indica 1.0.11. La build Android de esta versión se canceló antes de publicar un APK.
