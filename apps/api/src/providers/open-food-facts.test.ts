import { it, expect } from 'vitest';
import { normalizeOff } from './open-food-facts';
it('does not infer gluten freedom from ingredient analysis tags', () => {
  const p = normalizeOff({
    code: '12345678',
    product_name: 'Test',
    ingredients_analysis_tags: ['en:gluten-free'],
    nutriments: { sugars_100g: -1 },
  });
  expect(p?.gluten.declaration).toBeUndefined();
  expect(p?.nutrition.sugars).toBeUndefined();
});
it('normalizes source, claims, allergens and sodium', () => {
  const p = normalizeOff({
    code: '12345678',
    labels_tags: ['en:gluten-free'],
    allergens_tags: ['en:wheat'],
    nutriments: { sodium_100g: 0.4 },
    image_front_small_url: 'javascript:alert(1)',
  });
  expect(p?.gluten.declaration).toBe('sin gluten');
  expect(p?.gluten.allergens).toContain('wheat');
  expect(p?.nutrition.salt).toBe(1);
  expect(p?.imageUrl).toBeUndefined();
});
