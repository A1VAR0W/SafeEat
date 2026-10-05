# SafeEat 1.0.5

Se añade «Ocultar teclado» a los formularios y buscadores, con cierre al enviar un campo de una línea y al desplazar los formularios y resultados. Los campos de varias líneas mantienen los saltos de línea. El mapa permite cerrar el teclado al tocarlo y ofrece una acción independiente para borrar la búsqueda.

Buscar un restaurante conserva los demás restaurantes de la zona en el mapa. La búsqueda y el restaurante seleccionado se mantienen al mover el mapa o cambiar el zoom. La cámara se centra una vez por búsqueda; recuperar la conexión o volver a la app no vuelve a centrarla. Borrar la búsqueda o pulsar la ubicación permite continuar explorando otras zonas. Se respetan los filtros sin gluten.

Los textos compartidos, botones y filas con iconos pueden reducir su espacio y ocupar varias líneas. Las tarjetas crecen con su contenido; los paneles del mapa mantienen desplazamiento vertical cuando es necesario.

Incluye las mejoras de 1.0.4: motivos visibles, confirmación independiente con fecha y estado verde, y clasificación compartida para cadenas identificadas.

Validación: CI de GitHub completada; tipos, lint, formato, pruebas y compilaciones nativas. En Android 10 se comprobó el cierre del teclado en formularios de una y varias líneas. En Android 15 se comprobó la búsqueda con más de 100 marcadores, la conservación de selección y texto al desplazar y ampliar el mapa, y las tarjetas con texto grande de la app y escala del sistema de 1,4. El perfil gráfico antiguo del emulador Android 10 cerraba el renderizador del mapa tanto con 1.0.4 como con 1.0.5; la prueba del mapa se realizó en Android 15. No se ha probado esta versión en un iPhone físico.
