# Análisis de gluten (motor 1.2.0, reglas 2026.11.13)

Este documento explica cómo decide SafeEat 2.0 si un producto tiene gluten: el orden de
las pruebas, el catálogo de reglas, el papel de la IA, la evaluación y sus límites.
Complementa [SAFETY.md](SAFETY.md).

El motivo del cambio fue un error en preproducción. Kimbo «Patatas fritas sabor a
Toreras» (EAN 8410044005008) lleva «Patatas seleccionadas (64%), aceite de oliva (35%),
sal, dextrosa y aromas naturales». La app 2.0.2 lo mostraba en ámbar con «La IA ve
posible gluten», porque el modelo de 1.5B respondió «sí» a «dextrosa» o a «aromas
naturales». Ahora el resultado es **No verificado**, en gris, con este texto:

> No se han identificado ingredientes que declaren gluten. Sin embargo, no disponemos de
> información suficiente para confirmar que el producto sea apto para celíacos.
> La dextrosa no representa por sí misma un motivo de alerta por gluten. Los aromas
> naturales tampoco demuestran la presencia de gluten. Comprueba la información de
> alérgenos y posibles trazas del envase.

Hay tests en el motor, en el servidor (con el registro real de Open Food Facts) y en la
app que lo comprueban.

## 1. Orden de decisión

Cada nivel solo puede añadir precaución sobre el anterior. Ninguno de los niveles
posteriores puede deshacer una advertencia.

1. **Etiquetado del fabricante en vigor**: ingredientes, alérgenos, trazas y la mención
   «sin gluten», leídos de la fuente.
2. **Declaraciones y certificaciones verificables.** En la UE, la mención «sin gluten» está
   regulada por el Reglamento 828/2014 (≤ 20 mg/kg). SafeEat **no puede verificar** ningún
   certificado, así que el caso `VERIFIED_CERTIFICATION` existe pero nunca se produce.
3. **Motor determinista** (`crates/gluten-engine`):
   - El núcleo 1.1.0 es idéntico al de la API 1.x y pasa sus 1126 casos de referencia.
   - Encima va la capa de análisis `analyze()`, que clasifica cada ingrediente con el
     catálogo.
4. **Proveedores**, según su procedencia:
   - En Open Food Facts se distinguen los datos del fabricante y los de la comunidad.
   - USDA FoodData Central sigue el etiquetado de EE. UU. Ver la sección 5.
5. **IA local**: solo para los ingredientes que el catálogo no reconoce, y su respuesta
   nunca cambia el estado.
6. Si la información es insuficiente o contradictoria, el resultado es **NO VERIFICADO**.

## 2. Estados y casos

Los cuatro estados de la API no han cambiado, para no romper la app 1.0.x ni la 2.0.2.
Para distinguir los casos se ha añadido un campo opcional, `assessment.finding`.

| Estado               | Cuándo                                                        | Casos (`finding`)                                                                          |
| -------------------- | ------------------------------------------------------------- | ------------------------------------------------------------------------------------------ |
| `CONTAINS_GLUTEN`    | Hay pruebas positivas: un ingrediente o alérgeno con gluten declarado | `DECLARED_GLUTEN`, o `CONTRADICTORY` si las fuentes no coinciden                     |
| `MAY_CONTAIN_TRACES` | La fuente declara una advertencia de trazas (nunca una hipótesis de la IA) | `DECLARED_TRACES`, `CONTRADICTORY`                                                 |
| `GLUTEN_FREE`        | Declaración «sin gluten» del producto, sin contradicciones y con lista de ingredientes | `GLUTEN_FREE_CLAIM`                                                    |
| `UNKNOWN`            | Todo lo demás                                                 | `NO_GLUTEN_IDENTIFIED`, `AMBIGUOUS_INGREDIENT`, `INSUFFICIENT_INFORMATION`, `CONTRADICTORY` |

Para dar `GLUTEN_FREE`, la política es más estricta que la de la versión 1.x. Estos
casos se quedan en UNKNOWN y llevan su motivo:

- La mención «sin gluten» se refiere a un ingrediente y no al producto
  (p. ej. «harina sin gluten»). Motivo: `SCOPED_CLAIM`.
- Hay mención, pero falta la lista de ingredientes para contrastarla. Motivo:
  `CLAIM_WITHOUT_INGREDIENTS`.
- La mención convive con un derivado de cereal exento por ley. Motivo:
  `EXEMPTION_WITH_CLAIM`.
- La mención procede de una etiqueta aportada por un usuario. Motivo:
  `COMMUNITY_CLAIM_UNVERIFIED`.
- Los datos tienen más de 5 años. Motivo: `OLD_CLAIM`. A partir de 3 años solo se avisa,
  con `OLD_DATA`.
- La lectura de una foto con IA no confirma la mención. Motivo: `AI_ESTIMATE_DISAGREES`.

Si el dato procede de la comunidad de Open Food Facts:

- Se añade el motivo `COMMUNITY_DATA`.
- El «sello» pasa a ser una simple declaración, con el motivo `SEAL_NOT_VERIFIED`. Una
  foto frontal nunca cuenta como certificación.

## 3. Catálogo de reglas

Está en `crates/gluten-engine/src/catalog.rs` y se carga en memoria.

- **Contenido:** 51 reglas y una regla para palabras mal escritas.
- **Campos de cada regla:**
  - identificador;
  - términos en ES, EN, FR, IT, PT y DE, más algunos en CA y NL;
  - clase y posibles orígenes;
  - excepción legal y contexto que cambia la clase;
  - fuentes, versión y fecha de revisión;
  - explicación para el usuario.
- **Fuentes:**
  - Reglamento (UE) 1169/2011, art. 21 y anexo II.1;
  - Reglamento de Ejecución (UE) 828/2014;
  - AESAN;
  - FACE;
  - Coeliac UK.
- **Versión:** `RULES_VERSION = 2026.11.13`.
- **Comprobaciones de los tests:**
  - todas las reglas tienen fuentes y explicación;
  - las excepciones legales son estrechas;
  - una frase concreta gana a una palabra general («almidón de maíz» frente a «almidón»).

Clases, de más a menos grave. Un ingrediente toma la peor clase que reconoce:

| Clase                   | Ejemplos                                                                                                         | Efecto                                                     |
| ----------------------- | ---------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------- |
| `GLUTEN_SOURCE`         | trigo y sus variedades, espelta, kamut, cebada, malta, centeno, triticale, seitán, bulgur, cuscús                 | CONTIENE GLUTEN                                            |
| `LIKELY_GLUTEN`         | pan rallado, pasta, cerveza, levadura de cerveza, salsa de soja, sémola                                          | ambiguo como mínimo (el núcleo decide)                     |
| `OATS`                  | avena                                                                                                            | solo apta si el producto se declara sin gluten             |
| `NEEDS_ORIGIN`          | harina, almidón, almidón modificado, proteína vegetal, dextrina, cereales, extracto de levadura, fibra, sazonador | AMBIGUOUS_INGREDIENT                                       |
| `UNRECOGNIZED`          | palabras que el catálogo no conoce                                                                               | INSUFFICIENT_INFORMATION (y candidatas para la IA)         |
| `CONTEXT_DEPENDENT`     | aromas (naturales), especias, humo                                                                               | no son alerta en la UE; fuera de la UE pasan a NEEDS_ORIGIN |
| `LEGAL_EXEMPTION`       | jarabes de glucosa, dextrosa y maltodextrinas de trigo o cebada, destilados de cereales                           | no son prueba de gluten (anexo II.1)                       |
| `GENERALLY_COMPATIBLE`  | dextrosa, jarabe de glucosa, maltodextrina, edulcorantes, gasificantes, aditivos E                               | no son alerta                                              |
| `NATURALLY_GLUTEN_FREE` | patata, arroz, maíz, trigo sarraceno, aceite de oliva, sal, leche, carne, verdura                                | no son alerta, pero **no** hacen el producto apto: puede haber contaminación cruzada |

Cómo se trocea y se lee la lista (`items.rs`):

- **Ingredientes compuestos:** «chocolate (cacao, azúcar)» se separa en sus partes.
- **Paréntesis de origen:** «dextrosa (trigo)» se lee como «dextrosa de trigo».
- **Lo que no es un ingrediente:** porcentajes, menciones «sin gluten», avisos de trazas
  y frases como «consérvese…».
- **Errores de OCR:** una palabra a distancia de edición 1 de una palabra con gluten
  («cebaba», «espleta») se clasifica como de origen dudoso, nunca como segura. Si tiene
  8 letras o más, se admite distancia 2.

Excepciones legales:

- Se aplican tapando ese ingrediente y repitiendo el análisis.
- Nunca anulan los campos de alérgenos ni de trazas.
- Nunca dejan un producto en verde por sí solas.

Las otras versiones del texto de la misma ficha solo pueden añadir precaución. Son la
lista en el idioma original, la inglesa y la traducción al español. Si una de ellas da
gluten y la lista principal no, se muestra el aviso como `CONTRADICTORY` con el motivo
`TEXTS_DISAGREE`. Una mala traducción nunca sustituye al original.

## 4. La IA local (Qwen2.5-1.5B, sin servicios de pago)

Está en `apps/server/src/ai_check.rs`.

- **Cuándo se consulta:** solo si el único problema son ingredientes que el catálogo no
  reconoce (`INSUFFICIENT_INFORMATION`). Además deben cumplirse todas estas condiciones:
  - Open Food Facts reconoce todos los ingredientes;
  - ninguno está en la lista negra multilingüe;
  - hay como máximo 12 ingredientes, de 80 caracteres como máximo.
- **Cómo se pregunta:** un ingrediente cada vez. La respuesta debe ser JSON con un
  esquema cerrado, que se pide con `response_format` de llama.cpp y se vuelve a validar
  en el servidor con `deny_unknown_fields`:
  - `verdict`: `KNOWN_GLUTEN`, `KNOWN_SAFE_INGREDIENT`, `CONTEXT_DEPENDENT` o
    `INSUFFICIENT_INFORMATION`;
  - `cereal`: `WHEAT`, `BARLEY`, `RYE`, `OATS`, `SPELT`, `TRITICALE`, `KAMUT` o `NONE`.
- **Respuestas que se convierten en `MODEL_ERROR`:**
  - un error o un tiempo agotado;
  - una respuesta vacía o mal formada;
  - una respuesta incoherente, como `KNOWN_GLUTEN` sin cereal o «seguro» con cereal.

  `MODEL_ERROR` **nunca** se lee como gluten ni como seguro.
- **Resultado por producto** (`aiReview.outcome`):
  - `NO_GLUTEN_SEEN`;
  - `GLUTEN_SUSPECTED`;
  - `UNDETERMINED`;
  - `MODEL_ERROR`;
  - `PENDING`.
- **Qué puede mostrar:**
  - El estado **no cambia nunca**.
  - Una sospecha se muestra en gris como «sin confirmar», nunca en ámbar ni en rojo.
  - Para las apps 2.0.x ya instaladas, el campo antiguo `aiCheck` solo se envía como
    `VERIFIED` (orientativo, en gris) o `PENDING`. Esas apps pintaban `SUSPECTED` en ámbar,
    como si fueran trazas.
- **Textos:** los escribe SafeEat a partir del veredicto validado. No se muestra ningún
  texto libre del modelo.
- **Caché, por ingrediente:** tabla `ingredient_ai_verdicts`. La clave es
  sha256(ingrediente normalizado | modelo | versión del prompt | versión de las reglas).
- **Caché, por producto:** tabla `product_ai_reviews`. La clave es sha256(motor | reglas |
  modelo | prompt | ingredientes normalizados).
  - Si cambia la lista o cualquier versión, la entrada antigua deja de encontrarse.
  - Así, el `SUSPECTED` que tenía Kimbo en `product_ai_checks` ya no se lee.
  - Un `MODEL_ERROR` solo cuenta durante un día.
- **Límites:**
  - un trabajo a la vez;
  - 150 s por petición y 15 min por producto;
  - 3 intentos;
  - colas de 64 y 24 trabajos;
  - 32 tokens por respuesta.

## 5. Open Food Facts y USDA

- **Código de barras:** se comprueba que el código devuelto coincide con el pedido,
  ignorando los ceros a la izquierda. Si no coincide, se descarta.
- **Etiquetado UE o no UE:** se decide por `countries_tags`. Si no hay países, se aplica
  la lectura más estricta (no UE).
- **Procedencia:** se guarda si el dato es del fabricante o de la comunidad, y la fecha
  de modificación (`last_modified_t`).
- **Campos vacíos:** un campo de alérgenos o trazas vacío no prueba que no haya nada. Se
  trata como «sin dato».
- **Cambios de receta:** si la lista de ingredientes cambia entre dos descargas, se avisa
  con `INGREDIENTS_CHANGED`.
- **USDA FoodData Central** solo se consulta cuando Open Food Facts no da respuesta, y
  las fichas no se mezclan nunca:
  - Si marca o nombre no coinciden, se ignora.
  - Si avisa de gluten o trazas, se muestra el aviso con `SOURCES_DISAGREE`.
  - Su «sin gluten» nunca sustituye a una lista de Open Food Facts. Se anota como
    `OTHER_SOURCE_CLAIM`.

## 6. Versiones y caché

- `assessment.engineVersion` es ahora `1.2.0+rules.2026.11.13`.
- Un producto guardado con otra versión se trata así:
  - En lectura directa se vuelve a descargar.
  - En una búsqueda se muestra la vista previa nueva y se completa en segundo plano.
  - En favoritos y veredictos se marca como «datos sin actualizar».
- Las migraciones solo añaden tablas. `014_ai_reviews.sql` crea dos tablas nuevas, y las
  tablas 012 y 013 no se tocan.

## 7. Evaluación

La IA nunca se usa como referencia: los casos están etiquetados a mano.

```bash
cargo run --release -q -p safeeat-gluten-engine --example evaluate
```

Datos de la evaluación:

- **Casos:** 93 casos de `crates/gluten-engine/tests/fixtures/evaluation.json` y 50 listas
  reales de `apps/server/tests/fixtures/ai-gluten-cases.json`.
- **Cobertura de los casos:**
  - dextrosa, maltodextrina y jarabe de glucosa de trigo;
  - aromas, harina de trigo, malta de cebada, harina de arroz y trigo sarraceno;
  - avena con y sin «sin gluten»;
  - trazas, alérgenos vacíos y listas incompletas;
  - errores de OCR, etiquetas en varios idiomas y traducciones malas;
  - ingredientes compuestos y contradicciones.
- **Cómo se calcula:** cada caso se ejecuta 50 veces para medir la latencia. Los
  intervalos son de Clopper-Pearson al 95 %.
- **Fallo del ejemplo:** el ejemplo termina con error si hay un solo falso negativo
  peligroso.

Resultado del 13-11-2026 (143 casos: 83 con riesgo real y 60 sin él):

| Métrica                                                     | Resultado     | IC 95 %          |
| ----------------------------------------------------------- | ------------- | ---------------- |
| Falsos negativos peligrosos (verde o «no se identifican» con riesgo) | 0/83          | 0 – 4,35 %       |
| Sensibilidad (con riesgo y sin mensaje tranquilizador)      | 83/83         | 95,65 – 100 %    |
| Gluten → CONTIENE GLUTEN (rojo)                             | 49/75 (65 %)  | 53,5 – 76,0 %    |
| Trazas declaradas → ámbar                                   | 6/8           | 34,9 – 96,8 %    |
| Falsos positivos (rojo o ámbar sin riesgo)                  | 0/60          | 0 – 5,96 %       |
| UNKNOWN / abstención                                        | 83/143 (58 %) | 49,5 – 66,2 %    |
| Precisión de las respuestas favorables                      | 53/53         | 93,3 – 100 %     |
| Cobertura del catálogo (ingredientes)                       | 484/499 (97 %) | 95,1 – 98,3 %   |
| Productos que pedirían IA                                   | 1/143 (0,7 %) | 0,02 – 3,8 %     |
| Latencia del análisis determinista                          | p50 12 µs · p95 30 µs | —        |

**Advertencia:** el objetivo de menos de un 0,01 % de falsos negativos **no está
demostrado**. Con 0 fallos harían falta al menos 29 956 casos positivos etiquetados
para que la cota superior baje del 0,01 %. Hay además otras limitaciones:

- Las etiquetas de los 93 casos nuevos son de una sola persona y necesitan una segunda
  revisión humana.
- Los casos los escribió el mismo equipo que las reglas, así que el resultado es
  optimista. Hay que ampliarlo con productos reales elegidos al azar.
- Las 2 trazas que no salen en ámbar son frases que no nombran gluten: «Sin gluten no
  garantizado» y «Puede contener cereales». Salen como UNKNOWN con un ingrediente dudoso,
  nunca en verde.

En producción, el servidor mide sin datos personales y lo expone en
`GET /api/v1/admin/analysis-metrics`, solo para administradores:

- latencia p50 y p95;
- cobertura de las reglas;
- proporción de productos que necesitan IA;
- aciertos de caché;
- errores y latencia de la IA.

También lo escribe en el registro cada hora.

## 8. Casos que siguen siendo ambiguos (UNKNOWN a propósito)

- Almidón, almidón modificado, proteína vegetal, extracto de levadura y fibra sin
  origen.
- Sazonadores y mezclas sin lista.
- Avena sin la mención «sin gluten».
- Aromas y especias fuera de la UE.
- Ingredientes que el catálogo no conoce y que la IA no aclara.
- Listas incompletas o sin la sección de alérgenos.
- Fichas cuyos textos no coinciden.
