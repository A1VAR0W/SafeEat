import { describe, expect, it } from 'vitest';
import { RestaurantSchema } from '@safeeat/contracts';
import {
  directionsUrl,
  distanceLabel,
  hasCoordinates,
  restaurantFeatures,
  restaurantStatus,
} from './restaurant-presentation';

const restaurant = RestaurantSchema.parse({
  id: 'osm_n1',
  name: 'Café & Pan',
  address: 'Calle del Sol, Madrid',
  mapsUrl: 'https://www.openstreetmap.org/node/1',
  glutenSupport: 'UNKNOWN',
  evidence: [],
  source: 'catalog',
  fetchedAt: new Date().toISOString(),
  latitude: 40.4168,
  longitude: -3.7038,
});
describe('restaurant map and directions', () => {
  it('opens Google directions with the exact destination, without depending on provider mapsUrl', () => {
    const url = new URL(directionsUrl(restaurant));
    expect(url.origin).toBe('https://www.google.com');
    expect(url.pathname).toBe('/maps/dir/');
    expect(url.searchParams.get('api')).toBe('1');
    expect(url.searchParams.get('destination')).toBe('40.4168,-3.7038');
    expect(url.searchParams.has('origin')).toBe(false);
  });
  it('encodes restaurant name and address when coordinates are missing or invalid', () => {
    for (const latitude of [undefined, NaN, 91]) {
      const item = { ...restaurant, latitude };
      expect(hasCoordinates(item)).toBe(false);
      expect(new URL(directionsUrl(item)).searchParams.get('destination')).toBe(
        'Café & Pan, Calle del Sol, Madrid',
      );
    }
  });
  it('accepts zero coordinates and excludes invalid coordinates from native map data', () => {
    const zero = { ...restaurant, latitude: 0, longitude: 0 };
    const data = restaurantFeatures([
      zero,
      { ...restaurant, longitude: 181 },
      { ...restaurant, latitude: undefined },
    ]);
    expect(data.features).toHaveLength(1);
    expect(data.features[0]?.geometry.coordinates).toEqual([0, 0]);
    expect(data.features[0]?.properties?.id).toBe(restaurant.id);
  });
  it('does not label unknown or negative evidence as gluten free', () => {
    expect(restaurantStatus('UNKNOWN').label).toBe('No verificado');
    expect(restaurantStatus('NO_GLUTEN_FREE_OPTIONS').label).toBe('Sin opciones indicadas');
    expect(restaurantStatus('GLUTEN_FREE_OPTIONS').label).not.toBe(
      restaurantStatus('DEDICATED_GLUTEN_FREE').label,
    );
  });
  it('shows approximate straight-line distance only with coordinates and a user location', () => {
    expect(distanceLabel(restaurant, null)).toBeNull();
    expect(
      distanceLabel({ ...restaurant, latitude: undefined }, { latitude: 40, longitude: -3 }),
    ).toBeNull();
    expect(distanceLabel(restaurant, { latitude: 40.4268, longitude: -3.7038 })).toBe('1,1 km');
  });
});
