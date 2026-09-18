import { describe, expect, it } from 'vitest';
import { catalogRestaurant } from './catalog';

const base = {
  id: 'osm_n_123',
  name: 'Casa Uno',
  address: 'Calle Mayor, Madrid',
  latitude: 40.4,
  longitude: -3.7,
  website: null,
  phone: null,
  imported_at: new Date('2026-09-18T00:00:00Z'),
};

describe('restaurant catalog gluten labels', () => {
  it.each([
    ['only', 'DEDICATED_GLUTEN_FREE'],
    ['yes', 'GLUTEN_FREE_OPTIONS'],
    ['limited', 'UNKNOWN'],
    ['no', 'NO_GLUTEN_FREE_OPTIONS'],
    [null, 'UNKNOWN'],
  ])('maps diet:gluten_free=%s to %s', (gluten_tag, expected) => {
    expect(catalogRestaurant({ ...base, gluten_tag }).glutenSupport).toBe(expected);
  });
});
