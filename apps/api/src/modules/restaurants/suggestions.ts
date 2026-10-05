import type { RestaurantSuggestion } from '../../providers/types';

// Keep nearby restaurants and geographic destinations discoverable together.
export function combineSuggestions(
  local: RestaurantSuggestion[],
  external: RestaurantSuggestion[],
) {
  const result: RestaurantSuggestion[] = [];
  const seen = new Set<string>();
  for (let i = 0; i < Math.max(local.length, external.length); i++) {
    for (const item of [local[i], external[i]]) {
      if (!item) continue;
      const key = item.id ?? `${item.kind}:${item.label}:${item.latitude}:${item.longitude}`;
      if (seen.has(key)) continue;
      seen.add(key);
      result.push(item);
      if (result.length === 10) return result;
    }
  }
  return result;
}
