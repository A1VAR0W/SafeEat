import { expect, it } from 'vitest';
import type { Restaurant } from '@safeeat/contracts';
import { matchesRestaurantSearch } from './search';
const item = {
  name: 'McDonald’s - Centro',
  chain: { key: 'Q38076', name: "McDonald's" },
} as Restaurant;
it('matches names despite accents and punctuation and excludes other nearby restaurants', () => {
  expect(matchesRestaurantSearch(item, 'McDonalds')).toBe(true);
  expect(matchesRestaurantSearch({ name: 'Casa María' } as Restaurant, 'Casa Maria')).toBe(true);
  expect(matchesRestaurantSearch({ name: 'Burger King' } as Restaurant, 'McDonalds')).toBe(false);
});
it('uses the recognized chain identity instead of similar names and keeps zone searches broad', () => {
  expect(matchesRestaurantSearch(item, 'McDonalds', 'Q38076')).toBe(true);
  expect(
    matchesRestaurantSearch(
      { name: 'McDonalds independiente' } as Restaurant,
      'McDonalds',
      'Q38076',
    ),
  ).toBe(false);
  expect(matchesRestaurantSearch(item, 'restaurantes')).toBe(true);
});
