# SafeEat 1.0.7

Se reducen las actualizaciones del mapa nativo al escribir. Los callbacks y las listas de restaurantes conservan su referencia cuando los datos no cambian; abrir una tarjeta existente no vuelve a enviar todos los marcadores. En Android el cierre del teclado responde al inicio de un gesto del mapa, sin procesar cada movimiento táctil normal en JavaScript.

El autocompletado obtiene primero los restaurantes del catálogo y la comunidad. Después incorpora lugares del proveedor externo, con un máximo de 6,5 segundos de espera y cancelación al desconectarse el cliente. Esta segunda petición no bloquea las sugerencias locales. Un fallo externo conserva los resultados. Se priorizan sucursales cercanas y se intercalan destinos geográficos para que ciudades y calles no queden ocultas detrás de nombres de restaurantes. Elegir un restaurante conocido abre su tarjeta y centra la cámara una sola vez.

Las tarjetas de productos se reutilizan cuando sus datos no cambian; las listas virtualizadas acotan el trabajo inicial y las imágenes de los ocho primeros resultados se precargan en caché. Se conserva el contenido durante una nueva búsqueda. Las pestañas de Explorar y Guardados dejan de consultar cuando están ocultas. La limpieza de la caché de productos ya no recorre todos los registros después de cada petición de restaurantes. Cambiar una preferencia al mismo valor evita notificar innecesariamente a toda la interfaz.

Validación de código: tipos, lint, 144 pruebas unitarias y 36 pruebas de integración con PostgreSQL/PostGIS aislado. Incluye conservación de referencias, prioridad de destinos geográficos, sucursales cercanas y cancelación de proveedores lentos sin perder resultados locales.

La medición de Android 15 en emulador, con el mismo recorrido de escritura y ocho desplazamientos del mapa, dio 8,54% de frames tardíos en 1.0.6 y 8,18% en 1.0.7. El percentil 95 fue 18 ms en ambas; el 99 pasó de 20 a 25 ms. Una sola comparación no establece una mejora general de FPS. No se ha medido en dispositivos físicos ni en iPhone; la optimización elimina trabajo y esperas identificadas, pero no garantiza ausencia de pausas en todos los dispositivos o redes.
