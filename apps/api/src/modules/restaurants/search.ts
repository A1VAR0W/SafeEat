import type { Restaurant } from '@safeeat/contracts';
export function restaurantSearchName(value: string) {
  return value
    .normalize('NFD')
    .replace(/[\u0300-\u036f]/g, '')
    .toLowerCase()
    .replace(/[^a-z0-9]/g, '');
}
export function matchesRestaurantSearch(item: Restaurant, query: string, chainKey?: string) {
  if (/^(restaurantes?|cerca de m[ií])$/i.test(query)) return true;
  if (chainKey) return item.chain?.key === chainKey;
  const name = restaurantSearchName(query);
  return (
    !!name &&
    (restaurantSearchName(item.name).includes(name) ||
      restaurantSearchName(item.chain?.name ?? '').includes(name))
  );
}
