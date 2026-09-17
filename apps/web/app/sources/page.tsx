export default function Sources() {
  return (
    <>
      <h1>Fuentes y metodología</h1>
      <h2>Gluten</h2>
      <p>
        SafeEat busca declaraciones explícitas, cereales, alérgenos y advertencias de trazas. Las
        evidencias de riesgo prevalecen sobre una declaración contradictoria de ausencia de gluten.
        La falta de ingredientes peligrosos no demuestra que un alimento sea sin gluten.
      </p>
      <h2>Datos abiertos</h2>
      <p>
        Los productos proceden de <a href="https://world.openfoodfacts.org">Open Food Facts</a>, una
        base comunitaria bajo ODbL. Sus contenidos están bajo Database Contents License y sus
        imágenes bajo CC BY-SA. La información puede ser incompleta o incorrecta.
      </p>
      <p>
        Si está configurado, también se consulta{' '}
        <a href="https://fdc.nal.usda.gov/">USDA FoodData Central</a> por código de barras exacto.
        Las fotos de ingredientes aportadas a SafeEat se distinguen de las fichas de esas fuentes.
        Una aportación de usuario no certifica que el producto sea apto para celíacos.
      </p>
      <h2>Calidad nutricional</h2>
      <p>
        El indicador propio SafeEat utiliza valores por 100 g/ml: parte de 70 puntos, descuenta
        energía, azúcar, grasas saturadas y sal, y añade fibra y proteína. Se limita a 0–100 y se
        redondea a múltiplos de 5. Sin seis valores válidos no se calcula. No es un modelo
        clínicamente validado, no es Nutri-Score y no mide seguridad frente al gluten.
      </p>
      <h2>Restaurantes</h2>
      <p>
        OpenStreetMap o Google Maps aportan datos del lugar, pero no verifican la seguridad frente
        al gluten. SafeEat no deduce protocolos a partir del nombre o de opiniones.
      </p>
    </>
  );
}
