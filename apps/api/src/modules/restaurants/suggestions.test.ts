import { expect, it } from 'vitest';
import { combineSuggestions } from './suggestions';
it('geographic destinations stay discoverable when many restaurant names match a city', () => {
  const local = Array.from({ length: 5 }, (_, i) => ({
    id: `local-${i}`,
    label: `Madrid restaurant ${i}`,
    query: 'Madrid',
    kind: 'restaurant' as const,
  }));
  const place = {
    label: 'Madrid · España',
    query: 'restaurantes',
    kind: 'place' as const,
    latitude: 40.4,
    longitude: -3.7,
  };
  const first = combineSuggestions(local, [place, local[0]!]).slice(0, 5);
  expect(first).toContain(place);
  expect(first.filter((item) => item.id === 'local-0')).toHaveLength(1);
});
