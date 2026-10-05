import { expect, it } from 'vitest';
import type { Restaurant } from '@safeeat/contracts';
import { mergeRestaurantResults } from './restaurant-results';
const venue = {
  id: 'one',
  name: 'First',
  glutenSupport: 'UNKNOWN',
  evidence: [],
} as unknown as Restaurant;
it('opening a card does not replace unchanged marker data, even with a fresh equivalent response', () => {
  const base = [venue];
  expect(mergeRestaurantResults(base, [{ ...venue, evidence: [] }], { ...venue })).toBe(base);
});
it('keeps nearby markers and adds a selected restaurant that is outside the loaded results', () => {
  const other = { ...venue, id: 'other' };
  const merged = mergeRestaurantResults([venue], [other, other], other);
  expect(merged.map((item) => item.id)).toEqual(['one', 'other']);
});
it('new restaurant evidence wins over an older selection snapshot', () => {
  const updated = { ...venue, glutenSupport: 'GLUTEN_FREE_OPTIONS' as const };
  expect(mergeRestaurantResults([venue], [updated], venue)[0]?.glutenSupport).toBe(
    'GLUTEN_FREE_OPTIONS',
  );
});
