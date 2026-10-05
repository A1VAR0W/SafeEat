# SafeEat 1.0.6

Se retiran el botón y el icono «Ocultar teclado». Los formularios desplazan el contenido para mantener visibles el campo activo y el cursor mientras se escribe, también en campos de varias líneas. El desplazamiento se sincroniza con la apertura del teclado.

En iPhone se conserva el cierre interactivo al deslizar, sin forzar un cierre inmediato al iniciar el gesto. En Android se cierra al desplazar el contenido. Tocar fuera del campo, enviar un campo de una línea o interactuar con el mapa también permite cerrarlo. Los campos de varias líneas conservan los saltos de línea. La barra inferior se oculta mientras el teclado está abierto.

Se utiliza `react-native-keyboard-controller` 1.20.7 con las versiones de Reanimated y Worklets compatibles con Expo 55. El seguimiento del campo sustituye a la compensación manual de altura de 1.0.5. El formulario del escáner usa el mismo desplazamiento consciente del teclado.

Referencia de implementación: [guía de teclado de Expo](https://docs.expo.dev/guides/keyboard-handling/).
