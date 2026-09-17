import { it, expect } from 'vitest';
import { normalizeOff, normalizeSearchHit } from './open-food-facts';
it('treats search-index results as summaries without gluten or nutrition claims', () => {
  const item = normalizeSearchHit({
    code: '12345678',
    product_name: 'Arroz',
    brands: ['A', 'B'],
    labels_tags: ['en:gluten-free'],
    ingredients_text: 'Sin gluten',
    nutriments: { sugars_100g: 0 },
  });
  expect(item?.summaryOnly).toBe(true);
  expect(item?.brand).toBe('A, B');
  expect(item?.gluten.declaration).toBeUndefined();
  expect(item?.ingredients).toBeUndefined();
  expect(item?.nutrition.sugars).toBeUndefined();
});
it('uses the small thumbnail in discovery results', () => {
  const item = normalizeSearchHit({
    code: '12345678',
    product_name: 'Arroz',
    image_front_thumb_url: 'https://images.openfoodfacts.org/thumb.jpg',
    image_front_small_url: 'https://images.openfoodfacts.org/small.jpg',
  });
  expect(item?.imageUrl).toBe('https://images.openfoodfacts.org/thumb.jpg');
});
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
