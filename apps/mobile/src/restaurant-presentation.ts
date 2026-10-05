import type { Restaurant } from '@safeeat/contracts';

export type Coordinates = { latitude: number; longitude: number };
export type LocatedRestaurant = Restaurant & Coordinates;

export function hasCoordinates(item: Restaurant): item is LocatedRestaurant {
  return (
    typeof item.latitude === 'number' &&
    Number.isFinite(item.latitude) &&
    typeof item.longitude === 'number' &&
    Number.isFinite(item.longitude) &&
    Math.abs(item.latitude) <= 90 &&
    Math.abs(item.longitude) <= 180
  );
}

export function directionsUrl(item: Restaurant) {
  const destination = hasCoordinates(item)
    ? `${item.latitude},${item.longitude}`
    : [item.name, item.address].filter(Boolean).join(', ');
  return `https://www.google.com/maps/dir/?api=1&destination=${encodeURIComponent(destination)}`;
}

export function restaurantStatus(support: Restaurant['glutenSupport']) {
  switch (support) {
    case 'DEDICATED_GLUTEN_FREE':
      return {
        label: 'Solo sin gluten',
        detail:
          'El local figura como dedicado a comida sin gluten. Confirma con el personal cómo preparan los platos.',
        icon: 'leaf-outline',
        color: '#087653',
      } as const;
    case 'GLUTEN_FREE_OPTIONS':
      return {
        label: 'Opciones sin gluten',
        detail:
          'Hay opciones sin gluten indicadas. Pregunta qué platos pueden adaptar y cómo evitan la contaminación cruzada.',
        icon: 'restaurant-outline',
        color: '#087653',
      } as const;
    case 'NO_GLUTEN_FREE_OPTIONS':
      return {
        label: 'Sin opciones indicadas',
        detail: 'La información disponible indica que no ofrece opciones sin gluten.',
        icon: 'close-circle-outline',
        color: '#8C4C3A',
      } as const;
    default:
      return {
        label: 'No verificado',
        detail:
          'Todavía no tenemos información suficiente sobre las opciones sin gluten. Consulta al restaurante antes de pedir.',
        icon: 'alert-circle-outline',
        color: '#9A6800',
      } as const;
  }
}

export function restaurantFeatures(items: Restaurant[]): GeoJSON.FeatureCollection<GeoJSON.Point> {
  return {
    type: 'FeatureCollection',
    features: items.filter(hasCoordinates).map((item) => ({
      type: 'Feature',
      id: item.id,
      geometry: { type: 'Point', coordinates: [item.longitude, item.latitude] },
      properties: {
        id: item.id,
        name: item.name,
        color: restaurantStatus(item.glutenSupport).color,
      },
    })),
  };
}

export function distanceLabel(item: Restaurant, origin: Coordinates | null) {
  if (!origin || !hasCoordinates(item)) return null;
  const rad = Math.PI / 180;
  const a =
    Math.sin(((item.latitude - origin.latitude) * rad) / 2) ** 2 +
    Math.cos(origin.latitude * rad) *
      Math.cos(item.latitude * rad) *
      Math.sin(((item.longitude - origin.longitude) * rad) / 2) ** 2;
  const meters = 6371000 * 2 * Math.asin(Math.min(1, Math.sqrt(a)));
  return meters < 1000
    ? `${Math.max(10, Math.round(meters / 10) * 10)} m`
    : `${(meters / 1000).toFixed(1).replace('.', ',')} km`;
}
