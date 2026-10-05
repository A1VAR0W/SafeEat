import type { Restaurant } from '@safeeat/contracts';
import { replaceEqualDeep } from '@tanstack/react-query';

// Preserve the original array when selection adds no marker: the native map
// must not receive a replacement GeoJSON source just because a card opened.
export function mergeRestaurantResults(
  nearby: Restaurant[],
  named: Restaurant[],
  selected: Restaurant | null,
): Restaurant[] {
  const index = new Map(nearby.map((item, i) => [item.id, i]));
  let result: Restaurant[] | undefined;
  for (const item of named) {
    const i = index.get(item.id);
    const previous = i === undefined ? undefined : (result ?? nearby)[i];
    const next = previous ? replaceEqualDeep(previous, item) : item;
    if (next === previous) continue;
    result ??= [...nearby];
    if (i === undefined) {
      index.set(item.id, result.length);
      result.push(next);
    } else result[i] = next;
  }
  if (selected && !index.has(selected.id)) {
    result ??= [...nearby];
    result.push(selected);
  }
  return result ?? nearby;
}
