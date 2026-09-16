import { describe, it, expect } from 'vitest';
import { assessGluten } from './index';
describe('conservative gluten assessment', () => {
  it.each([
    ['SIN GLUTEN', 'GLUTEN_FREE'],
    ['GLUTEN-FREE', 'GLUTEN_FREE'],
    ['ｓｉｎ ｇｌｕｔｅｎ', 'GLUTEN_FREE'],
    ['No contiene gluten', 'GLUTEN_FREE'],
    ['Ingredientes: trigo, agua', 'CONTAINS_GLUTEN'],
    ['wheat flour', 'CONTAINS_GLUTEN'],
    ['barley', 'CONTAINS_GLUTEN'],
    ['rye', 'CONTAINS_GLUTEN'],
    ['espelta', 'CONTAINS_GLUTEN'],
    ['KAMUT', 'CONTAINS_GLUTEN'],
    ['triticale', 'CONTAINS_GLUTEN'],
    ['extracto de malta', 'CONTAINS_GLUTEN'],
    ['PUEDE CONTENER TRAZAS DE GLUTEN', 'MAY_CONTAIN_TRACES'],
    ['may contain traces of wheat', 'MAY_CONTAIN_TRACES'],
    ['Sin gluten. Puede contener trigo', 'MAY_CONTAIN_TRACES'],
    ['Sin gluten; ingredientes: cebada', 'CONTAINS_GLUTEN'],
    ['sin gluten, trigo', 'CONTAINS_GLUTEN'],
    ['trigo, puede contener leche', 'CONTAINS_GLUTEN'],
    ['Puede contener leche; ingredientes: trigo', 'CONTAINS_GLUTEN'],
    ['avena', 'UNKNOWN'],
    ['Sin gluten. Avena', 'GLUTEN_FREE'],
    ['maltodextrina, buckwheat, trigo sarraceno', 'UNKNOWN'],
    ['Sin gluten. No apto para celíacos', 'UNKNOWN'],
    ['No declara sin gluten', 'UNKNOWN'],
    ['No pone sin gluten', 'UNKNOWN'],
    ['No apto para celíacos', 'UNKNOWN'],
    ['Puede contener cereales', 'UNKNOWN'],
    ['Puede contener trigo', 'MAY_CONTAIN_TRACES'],
    ['Puede contener gluten', 'MAY_CONTAIN_TRACES'],
    ['Sin gluten según receta, puede contener trigo', 'MAY_CONTAIN_TRACES'],
    ['El fabricante no garantiza ausencia de gluten', 'UNKNOWN'],
    ['No se ha comprobado si contiene gluten', 'UNKNOWN'],
    ['Maltodextrina', 'UNKNOWN'],
    ['Trigo sarraceno', 'UNKNOWN'],
    ['Buckwheat', 'UNKNOWN'],
    ['Oats', 'UNKNOWN'],
    ['Puede contener trazas de avena', 'MAY_CONTAIN_TRACES'],
    ['Sin gluten según receta', 'UNKNOWN'],
    ['Antes era sin gluten', 'UNKNOWN'],
    ['Sin gluten. Sin comprobar', 'UNKNOWN'],
    ['Gluten free. Not tested', 'UNKNOWN'],
    ['Sin gluten. Cereales no especificados', 'UNKNOWN'],
    ['Sin gluten. El fabricante no garantiza ausencia de gluten', 'UNKNOWN'],
    ['Sin gluten. No se ha comprobado si contiene gluten', 'UNKNOWN'],
    ['Sin gluten. Puede contener cereales', 'UNKNOWN'],
    ['Sin gluten. Puede contener', 'UNKNOWN'],
    ['Does not contain gluten', 'GLUTEN_FREE'],
    ['May contain\nwheat', 'MAY_CONTAIN_TRACES'],
    ['maltodextrin, buckwheat', 'UNKNOWN'],
    ['almidón, agua', 'UNKNOWN'],
    ['', 'UNKNOWN'],
    ['sin trigo', 'UNKNOWN'],
    ['no es sin gluten', 'UNKNOWN'],
    ['not gluten free', 'UNKNOWN'],
    ['gluten free?', 'UNKNOWN'],
    ['opción sin gluten', 'UNKNOWN'],
    ['No podemos garantizar que sea sin gluten', 'UNKNOWN'],
    ['No apto para dietas sin gluten', 'UNKNOWN'],
    ['may contain gluten. gluten free', 'MAY_CONTAIN_TRACES'],
  ])('%s → %s', (ingredients, expected) => {
    const result = assessGluten({ ingredients, source: 'test' });
    expect(result.status).toBe(expected);
    expect(result.reasons.length).toBeGreaterThan(0);
  });
  it('never trusts unreviewed OCR safety claims', () =>
    expect(assessGluten({ ingredients: 'sin gluten', source: 'OCR', origin: 'ocr' }).status).toBe(
      'UNKNOWN',
    ));
  it('provider allergens override claim', () =>
    expect(
      assessGluten({ declaration: 'sin gluten', allergens: 'wheat', source: 'OFF' }).status,
    ).toBe('CONTAINS_GLUTEN'));
  it('structured traces are never a positive safe claim', () =>
    expect(
      assessGluten({ traces: 'wheat', declaration: 'gluten free', source: 'OFF' }).status,
    ).toBe('MAY_CONTAIN_TRACES'));
});
