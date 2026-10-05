# SafeEat 1.0.10

Las revisiones de restaurantes permiten elegir entre «Solo este restaurante» y «Todos los locales». Se utiliza la identidad de la cadena cuando existe; para fichas sin identidad de marca se ofrece un grupo por nombre normalizado coincidente si hay al menos dos locales registrados. Hamburguesa Nostra tiene nueve locales de este tipo en el catálogo actual. La opción por defecto de la nueva interfaz es local. El servidor deriva el identificador del grupo, sin aceptar una clave arbitraria enviada por el cliente.

El alcance se guarda en `restaurant_gluten_reports.scope`. Las revisiones locales no se amplían durante nuevas importaciones del catálogo. Las revisiones ya compartidas conservan su alcance previo; la migración de ese metadato conserva sus revisiones y confirmaciones existentes. Cambiar de local a cadena o viceversa actualiza la revisión e invalida las confirmaciones de la versión anterior. Los informes compartidos también participan en los filtros del mapa y pueden confirmarse desde otra sucursal.

Se crean las tablas `roles` y `user_roles`, con el rol `authorized_celiac` y nombre «Celiacos autorizados». Su asignación se realiza administrativamente en BBDD: no existe una opción pública para autoasignarlo. Las revisiones de autores con ese rol se muestran automáticamente verificadas en restaurantes y productos, con fecha y distintivo verde. Se verifica el rol vigente al leer; revocarlo retira la verificación automática, conservando las confirmaciones manuales que correspondan. Las fichas de producto distinguen revisiones verificadas de pendientes sin sobrescribir el análisis de ingredientes ni ocultar contradicciones.

Android entra directamente en Explorar: se elimina la ruta inicial de redirección y la pantalla principal pasa a ser el índice del grupo de pestañas. El enlace antiguo `safeeat://home` se normaliza a la nueva entrada. Se configura `expo-splash-screen` compatible con Expo 55 y se conserva el splash nativo hasta que el navegador está listo. Referencia: [documentación de Expo SplashScreen](https://docs.expo.dev/versions/latest/sdk/splash-screen/).

Validación de código: tipos, lint, formato, 146 pruebas unitarias y 40 de integración en PostgreSQL/PostGIS aislado. Incluye alcance local y compartido para cadenas identificadas y grupos por nombre, cambios de alcance, exclusión de nombres distintos, permisos, edición y revocación de roles, rechazo de autoasignación, verificación automática en productos y restaurantes y conservación del flujo normal de confirmaciones.

Producción: migración aplicada tras copia de seguridad y rol solicitado asignado a la cuenta existente. El smoke de producción pasa. La APK Release, código 86, conserva el certificado de las versiones anteriores y se instala como actualización. En Android 15 se comprobó el inicio desde el lanzador en Explorar, el enlace anterior a home, ausencia de título index y la elección entre alcance local y los nueve locales de Hamburguesa Nostra, sin publicar revisiones de prueba en restaurantes reales. Las capturas no muestran desbordamiento del selector y no se registraron errores de ejecución durante esas pruebas.

La IPA de producción compiló en GitHub Actions. Ambas plataformas pasan la validación de configuración y bundle embebidos; los enlaces públicos y el manifiesto Android corresponden a 1.0.10. La fuente iOS tiene el tamaño y la versión del nuevo archivo. iPhone pendiente de prueba física.

Administración del rol, desde una sesión autorizada de PostgreSQL, usando una variable `email` definida en psql:

```sql
INSERT INTO user_roles(user_id,role_code)
SELECT id,'authorized_celiac' FROM users WHERE lower(email)=lower(:'email')
ON CONFLICT DO NOTHING;
-- Revocar únicamente el rol de esa cuenta:
DELETE FROM user_roles WHERE role_code='authorized_celiac'
AND user_id=(SELECT id FROM users WHERE lower(email)=lower(:'email'));
```
