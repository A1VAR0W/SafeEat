// Generates crates/gluten-engine/tests/golden.json from the TypeScript engine so the
// Rust port can prove identical behaviour. Run: npx tsx scripts/export-gluten-golden.mts
import { writeFileSync } from 'node:fs';
import { assessGluten, type GlutenInput } from '../packages/gluten-engine/src/index';
import { scoreNutrition } from '../packages/nutrition-engine/src/index';

const texts = [
  'SIN GLUTEN', 'GLUTEN-FREE', 'ｓｉｎ ｇｌｕｔｅｎ', 'No contiene gluten', 'Ingredientes: trigo, agua',
  'wheat flour', 'barley', 'rye', 'espelta', 'KAMUT', 'triticale', 'extracto de malta',
  'PUEDE CONTENER TRAZAS DE GLUTEN', 'may contain traces of wheat', 'Sin gluten. Puede contener trigo',
  'Sin gluten; ingredientes: cebada', 'sin gluten, trigo', 'trigo, puede contener leche',
  'Puede contener leche; ingredientes: trigo', 'avena', 'Sin gluten. Avena',
  'maltodextrina, buckwheat, trigo sarraceno', 'Sin gluten. No apto para celíacos', 'No declara sin gluten',
  'No pone sin gluten', 'No apto para celíacos', 'Puede contener cereales', 'Puede contener trigo',
  'Puede contener gluten', 'Sin gluten según receta, puede contener trigo',
  'El fabricante no garantiza ausencia de gluten', 'No se ha comprobado si contiene gluten', 'Maltodextrina',
  'Trigo sarraceno', 'Buckwheat', 'Oats', 'Puede contener trazas de avena', 'Sin gluten según receta',
  'Antes era sin gluten', 'Sin gluten. Sin comprobar', 'Gluten free. Not tested',
  'Sin gluten. Cereales no especificados', 'Sin gluten. El fabricante no garantiza ausencia de gluten',
  'Sin gluten. No se ha comprobado si contiene gluten', 'Sin gluten. Puede contener cereales',
  'Sin gluten. Puede contener', 'Does not contain gluten', 'May contain\nwheat', 'maltodextrin, buckwheat',
  'almidón, agua', '', '   ', 'sin trigo', 'no es sin gluten', 'not gluten free', 'gluten free?',
  'opción sin gluten', 'No podemos garantizar que sea sin gluten', 'No apto para dietas sin gluten',
  'may contain gluten. gluten free',
  'Harina de TRIGO (gluten), agua, levadura, sal. Puede contener trazas de soja y sésamo.',
  'Arroz, maíz, azúcar, sal, extracto de malta de cebada. Contiene: gluten.',
  'Leche entera, fermentos lácticos.',
  'Copos de avena sin gluten (100%).',
  'Avena integral, puede contener trazas de trigo',
  'Elaborado en una fábrica que utiliza trigo; ingredientes: arroz, agua',
  'Fabricado en instalaciones que procesan cereales con gluten',
  'Contaminación cruzada posible con cebada',
  'Ingredientes: almidón de trigo sin gluten',
  'Sin gluten — apto para celíacos',
  'Libre de gluten. Certificado por FACE.',
  'Gluten free; made in a facility that processes wheat',
  'Processed in a facility that also handles nuts. Ingredients: rice, salt',
  'Semolina (wheat), water, egg',
  'contiene:trigo',
  'Contains: wheat, milk',
  'Puede contener: trazas de leche. Alérgenos: trigo',
  'Malt vinegar',
  'spelt flour; khorasan',
  'Ingredientes: patata, aceite de girasol, sal. Sin gluten. Sin lactosa.',
  'Ingredientes: patata, aceite de girasol, sal. Sin gluten? Consultar',
  'pan rallado (trigo), huevo; puede contener mostaza',
  'Not suitable for coeliacs',
  'Possibly contains gluten',
  'sin gluten, excepto la salsa',
  'Gluten-free option available',
  'Ingredients: oats, honey. Gluten free oats.',
  'cerveza sin gluten (malta de cebada desglutinizada)',
  'no contiene trigo ni cebada',
  'harina de arroz, harina de maíz, almidón de patata, goma xantana',
  'ÁGUA, HARINA DE TRIGO, ACEITE DE OLIVA',
  'Ingredientes: azúcar, cacao. Puede contener trazas de frutos de cáscara, leche y gluten.',
  'ingredientes: azucar; alergenos: ninguno',
  'Trazas de: avena',
  'may contain',
  'traces of',
];

const cases: { input: GlutenInput; output: unknown }[] = [];
const push = (input: GlutenInput) => cases.push({ input, output: assessGluten(input) });
for (const text of texts) {
  push({ ingredients: text, source: 'test' });
  push({ ingredients: text, source: 'OFF', sourceUrl: 'https://world.openfoodfacts.org/product/1', origin: 'provider' });
  push({ ingredients: text, source: 'OCR', origin: 'ocr', reviewed: false });
  push({ ingredients: text, source: 'OCR', origin: 'ocr', reviewed: true });
  push({ declaration: text, source: 'decl' });
  push({ traces: text, source: 'tr', origin: 'manual' });
  push({ allergens: text, source: 'al', origin: 'provider' });
}
push({ declaration: 'sin gluten', allergens: 'wheat', source: 'OFF' });
push({ traces: 'wheat', declaration: 'gluten free', source: 'OFF' });
push({ ingredients: 'arroz', allergens: 'en:milk', traces: 'en:gluten, en:nuts', declaration: 'sin gluten', source: 'OFF', origin: 'provider' });
push({ ingredients: 'arroz, sal', allergens: '', traces: '', declaration: 'sin gluten', source: 'OFF', origin: 'provider' });
push({ ingredients: 'arroz, sal', allergens: 'en:gluten', source: 'OFF', origin: 'provider' });
push({ source: 'empty' });

const nutrition = [
  {},
  { energyKcal: 250, saturatedFat: 1, sugars: 3, salt: 0.2, fibre: 6, protein: 12 },
  { energyKcal: 550, saturatedFat: 12, sugars: 40, salt: 1.2, fibre: 1, protein: 5 },
  { energyKcal: 0, saturatedFat: 0, sugars: 0, salt: 0, fibre: 0, protein: 0 },
  { energyKcal: 900, saturatedFat: 50, sugars: 50, salt: 10, fibre: 0, protein: 0 },
  { energyKcal: 120, saturatedFat: 0.4, sugars: 2.5, salt: 0.45, fibre: 3, protein: 7.5 },
  { energyKcal: 1000, saturatedFat: 1, sugars: 1, salt: 1, fibre: 1, protein: 1 },
  { energyKcal: 100, saturatedFat: -1, sugars: 1, salt: 1, fibre: 1, protein: 1 },
].map((input) => ({ input, output: scoreNutrition(input) }));

writeFileSync(
  new URL('../crates/gluten-engine/tests/golden.json', import.meta.url),
  JSON.stringify({ gluten: cases, nutrition }, null, 1) + '\n',
);
console.log(`Wrote ${cases.length} gluten and ${nutrition.length} nutrition cases`);
