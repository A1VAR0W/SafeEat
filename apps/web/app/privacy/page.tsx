export default function Privacy() {
  return (
    <>
      <h1>Privacidad</h1>
      <p>
        SafeEat almacena el correo electrónico, una contraseña cifrada mediante hash y las sesiones
        de acceso necesarias para tu cuenta. Puedes eliminarla desde Perfil; se eliminarán sus
        favoritos, grupos, etiquetas, fotos aportadas y enlaces personales.
      </p>
      <p>
        La lectura de una etiqueta personal se realiza en el dispositivo y su foto se elimina
        después. Si decides aportar la foto de ingredientes de un producto con código de barras,
        enviamos al servidor la imagen y el texto que has revisado. Otros usuarios podrán
        consultarlos hasta que elimines tu cuenta. La imagen se reduce y se vuelve a codificar sin
        metadatos antes de guardarse. Si compartes una etiqueta personal, cualquier persona con el
        enlace podrá leerla.
      </p>
      <p>
        No incorporamos publicidad ni herramientas de analítica. Las consultas de productos se
        envían a Open Food Facts y, cuando esté configurado, a USDA FoodData Central. Las consultas
        de restaurantes se envían a OpenStreetMap/Photon o a Google Maps según la configuración. Tu
        ubicación solo se comparte si eliges buscar cerca de ti.
      </p>
      <p>
        La caché local puede borrarse desde Ajustes. Los datos eliminados pueden permanecer hasta 14
        días en copias de seguridad restringidas. Las búsquedas no se registran en los logs de la
        aplicación.
      </p>
      <p>
        Antes de publicar en tiendas, el operador debe completar la identidad legal, el contacto de
        privacidad y las condiciones aplicables a su distribución.
      </p>
    </>
  );
}
