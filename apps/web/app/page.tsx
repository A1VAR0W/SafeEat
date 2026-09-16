export default function Home() {
  return (
    <>
      <p className="eyebrow">Tu compañero en cada etiqueta</p>
      <h1>
        Más información.
        <br />
        Menos dudas.
      </h1>
      <p className="lede">
        Escanea un producto, revisa sus ingredientes y descubre qué evidencia hay sobre el gluten.
        Porque una respuesta útil también explica el porqué.
      </p>
      <a className="button" href="safeeat://home">
        Abrir SafeEat
      </a>
      <div className="grid">
        <section className="card">
          <h2>Primero, la etiqueta.</h2>
          <p>
            Códigos de barras y lectura de ingredientes, con resultados que distinguen gluten,
            trazas e información insuficiente.
          </p>
        </section>
        <section className="card">
          <h2>Dos cosas distintas.</h2>
          <p>
            La información sobre gluten y la calidad nutricional se muestran por separado. Sin
            gluten no significa más saludable.
          </p>
        </section>
        <section className="card">
          <h2>Comparte con contexto.</h2>
          <p>
            Abre un enlace de SafeEat para consultar el producto o restaurante, sus fuentes y las
            limitaciones de la información.
          </p>
        </section>
      </div>
    </>
  );
}
