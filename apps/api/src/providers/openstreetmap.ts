import { z } from 'zod';
import type { Restaurant } from '@safeeat/contracts';
import type { RestaurantProvider, RestaurantSearch, RestaurantSuggestion } from './types';
import { providerJson, ProviderBudget, safeHttpUrl } from './http';

const feature = z.object({
  properties: z.object({
    osm_type: z.enum(['N', 'W', 'R']),
    osm_id: z.number(),
    name: z.string().optional(),
    street: z.string().optional(),
    housenumber: z.string().optional(),
    city: z.string().optional(),
    state: z.string().optional(),
    country: z.string().optional(),
    osm_key: z.string().optional(),
    osm_value: z.string().optional(),
  }),
  geometry: z.object({ coordinates: z.tuple([z.number(), z.number()]) }),
});
const features = z.object({ features: z.array(feature) });
const osmElement = z.object({
  type: z.enum(['node', 'way', 'relation']),
  id: z.number(),
  lat: z.number().optional(),
  lon: z.number().optional(),
  center: z.object({ lat: z.number(), lon: z.number() }).optional(),
  tags: z.record(z.string(), z.string()).optional(),
});
const elementEnvelope = z.object({ elements: z.array(osmElement) });
const typeCode = { N: 'node', W: 'way', R: 'relation' } as const;
const codeType = { n: 'node', w: 'way', r: 'relation' } as const;
const foodTypes = 'restaurant|cafe|fast_food|bar|pub|food_court|biergarten';
const foodType = new RegExp(`^(${foodTypes})$`);
const foodShopTypes = new Set(['bakery', 'pastry', 'convenience']);
const glutenName = /sin\s+gluten|gluten\s*free|celi[a-záí]+/i;
function isFoodFeature(properties: z.infer<typeof feature>['properties']) {
  return (
    (properties.osm_key === 'amenity' && foodType.test(properties.osm_value ?? '')) ||
    (properties.osm_key === 'shop' && foodShopTypes.has(properties.osm_value ?? ''))
  );
}
function isFoodTags(tags: Record<string, string>) {
  return (
    (tags.amenity !== undefined && foodType.test(tags.amenity)) ||
    (tags.shop !== undefined && foodShopTypes.has(tags.shop))
  );
}
function distanceKm(a: { lat: number; lon: number }, b: { lat: number; lon: number }) {
  const radians = Math.PI / 180;
  const lat = (b.lat - a.lat) * radians;
  const lon = (b.lon - a.lon) * radians;
  const value =
    Math.sin(lat / 2) ** 2 +
    Math.cos(a.lat * radians) * Math.cos(b.lat * radians) * Math.sin(lon / 2) ** 2;
  return 12742 * Math.atan2(Math.sqrt(value), Math.sqrt(1 - value));
}
function restaurant(input: {
  type: 'node' | 'way' | 'relation';
  id: number;
  name: string;
  address: string;
  lat?: number;
  lon?: number;
  tags?: Record<string, string>;
  glutenNameMatch?: boolean;
}): Restaurant {
  const mapsUrl = `https://www.openstreetmap.org/${input.type}/${input.id}`;
  const glutenTag = input.tags?.['diet:gluten_free'];
  const glutenSupport =
    glutenTag === 'only'
      ? 'DEDICATED_GLUTEN_FREE'
      : glutenTag === 'yes'
        ? 'GLUTEN_FREE_OPTIONS'
        : glutenTag === 'no'
          ? 'NO_GLUTEN_FREE_OPTIONS'
          : 'UNKNOWN';
  return {
    id: `osm_${input.type[0]}_${input.id}`,
    name: input.name,
    address: input.address,
    latitude: input.lat,
    longitude: input.lon,
    website: safeHttpUrl(input.tags?.website || input.tags?.['contact:website']),
    phone: input.tags?.phone || input.tags?.['contact:phone'],
    mapsUrl,
    glutenSupport,
    chain: /^Q\d+$/.test(input.tags?.['brand:wikidata'] ?? '')
      ? { key: input.tags!['brand:wikidata']!, name: input.tags?.brand || input.name }
      : undefined,
    evidence: [
      {
        source: 'OpenStreetMap',
        kind: 'uncertainty',
        text: input.glutenNameMatch
          ? 'El nombre de la ficha menciona opciones sin gluten. Confirma ingredientes y contaminación cruzada con el establecimiento.'
          : glutenSupport === 'DEDICATED_GLUTEN_FREE'
            ? 'La ficha indica que el local ofrece prácticamente todo sin gluten. Confirma la preparación antes de acudir.'
            : glutenSupport === 'GLUTEN_FREE_OPTIONS'
              ? 'La ficha indica opciones sin gluten. Confirma ingredientes y contaminación cruzada.'
              : glutenSupport === 'NO_GLUTEN_FREE_OPTIONS'
                ? 'La ficha indica que no ofrece opciones sin gluten.'
                : 'Aún no hay información sobre opciones sin gluten.',
      },
    ],
    source: 'OpenStreetMap · ODbL',
    attributions: ['© OpenStreetMap contributors'],
    fetchedAt: new Date().toISOString(),
  };
}
function fromFeature(f: z.infer<typeof feature>): Restaurant | null {
  const p = f.properties;
  if (!p.name || !isFoodFeature(p)) return null;
  return restaurant({
    type: typeCode[p.osm_type],
    id: p.osm_id,
    name: p.name,
    address: [p.street, p.housenumber, p.city, p.state, p.country].filter(Boolean).join(', '),
    lat: f.geometry.coordinates[1],
    lon: f.geometry.coordinates[0],
  });
}
function glutenFeature(f: z.infer<typeof feature>): Restaurant | null {
  const p = f.properties;
  if (!p.name || !glutenName.test(p.name) || !isFoodFeature(p)) return null;
  return restaurant({
    type: typeCode[p.osm_type],
    id: p.osm_id,
    name: p.name,
    address: [p.street, p.housenumber, p.city, p.state, p.country].filter(Boolean).join(', '),
    lat: f.geometry.coordinates[1],
    lon: f.geometry.coordinates[0],
    glutenNameMatch: true,
  });
}
export class OpenStreetMapProvider implements RestaurantProvider {
  private budget = new ProviderBudget(45);
  private cache = new Map<string, { at: number; value: Restaurant[] | RestaurantSuggestion[] }>();
  constructor(private userAgent: string) {}
  private async photon(
    q: string,
    tag?: string,
    limit = 10,
    location?: { lat: number; lon: number },
  ) {
    this.budget.take();
    // Photon does not support Spanish as a language code. Omitting `lang`
    // lets the service use its default language while keeping the query
    // compatible with the public endpoint.
    const params = new URLSearchParams({ q, limit: String(limit) });
    if (tag) params.set('osm_tag', tag);
    if (location) {
      params.set('lat', String(location.lat));
      params.set('lon', String(location.lon));
    }
    return features.parse(
      await providerJson(`https://photon.komoot.io/api/?${params}`, {
        headers: { 'User-Agent': this.userAgent },
      }),
    ).features;
  }
  private cached<T extends Restaurant[] | RestaurantSuggestion[]>(key: string): T | undefined {
    const item = this.cache.get(key);
    return item && Date.now() - item.at < 3600000 ? (item.value as T) : undefined;
  }
  private store<T extends Restaurant[] | RestaurantSuggestion[]>(key: string, value: T): T {
    if (this.cache.size > 150) this.cache.delete(this.cache.keys().next().value!);
    this.cache.set(key, { at: Date.now(), value });
    return value;
  }
  async suggest(
    query: string,
    location?: { latitude: number; longitude: number },
  ): Promise<RestaurantSuggestion[]> {
    const lat = location && Math.round(location.latitude * 100) / 100;
    const lon = location && Math.round(location.longitude * 100) / 100;
    const key = `suggest:${query.toLocaleLowerCase()}:${lat ?? ''}:${lon ?? ''}`;
    const old = this.cached<RestaurantSuggestion[]>(key);
    if (old) return old;
    const found = await this.photon(
      query,
      undefined,
      12,
      lat !== undefined && lon !== undefined ? { lat, lon } : undefined,
    );
    const seen = new Set<string>();
    return this.store(
      key,
      found
        .filter((f) => f.properties.name)
        .filter((f) => {
          const identity = `${f.properties.name}:${f.properties.city ?? ''}:${f.properties.osm_key ?? ''}`;
          if (seen.has(identity)) return false;
          seen.add(identity);
          return true;
        })
        .map((f) => {
          const food =
            f.properties.osm_key === 'amenity' && foodType.test(f.properties.osm_value ?? '');
          return {
            label: [f.properties.name, f.properties.city || f.properties.country]
              .filter(Boolean)
              .join(' · '),
            kind: food ? ('restaurant' as const) : ('place' as const),
            query: food ? f.properties.name! : 'restaurantes',
            latitude: f.geometry.coordinates[1],
            longitude: f.geometry.coordinates[0],
          };
        }),
    );
  }
  private async nearby(lat: number, lon: number, glutenFree = false) {
    this.budget.take();
    const tag = glutenFree ? '["diet:gluten_free"~"^(yes|only|limited)$"]' : '';
    // Public Overpass instances can time out on a 12 km dietary query. Four km
    // remains useful for map discovery while consistently returning before the
    // public production proxy deadline.
    const radius = glutenFree ? 4000 : 1800;
    const q = `[out:json][timeout:7];nwr[amenity~"^(${foodTypes})$"]${tag}(around:${radius},${lat},${lon});out center ${glutenFree ? 60 : 50};`;
    const data = elementEnvelope.parse(
      await providerJson(
        'https://overpass-api.de/api/interpreter',
        {
          method: 'POST',
          headers: {
            'User-Agent': this.userAgent,
            'Content-Type': 'application/x-www-form-urlencoded',
          },
          body: new URLSearchParams({ data: q }).toString(),
        },
        7500,
      ),
    );
    return data.elements
      .filter((e) => e.tags?.name)
      .map((e) =>
        restaurant({
          type: e.type,
          id: e.id,
          name: e.tags!.name!,
          address: [e.tags?.['addr:street'], e.tags?.['addr:housenumber'], e.tags?.['addr:city']]
            .filter(Boolean)
            .join(', '),
          lat: e.lat ?? e.center?.lat,
          lon: e.lon ?? e.center?.lon,
          tags: e.tags,
        }),
      );
  }
  private async namedGlutenFreeNearby(lat: number, lon: number) {
    const queries = ['sin gluten', 'gluten free', 'celiaco'];
    const results = await Promise.all(
      queries.map((query) => this.photon(query, undefined, 30, { lat, lon })),
    );
    const seen = new Set<string>();
    return results
      .flat()
      .map(glutenFeature)
      .filter((item): item is Restaurant => !!item)
      .filter((item) => {
        if (item.latitude === undefined || item.longitude === undefined) return false;
        if (distanceKm({ lat, lon }, { lat: item.latitude, lon: item.longitude }) > 25)
          return false;
        if (seen.has(item.id)) return false;
        seen.add(item.id);
        return true;
      })
      .slice(0, 30);
  }
  private async nearbyFromPhoton(lat: number, lon: number) {
    const found = await this.photon('restaurantes', undefined, 50, { lat, lon });
    return found
      .map(fromFeature)
      .filter((item): item is Restaurant => !!item)
      .filter((item) => {
        if (item.latitude === undefined || item.longitude === undefined) return false;
        return distanceKm({ lat, lon }, { lat: item.latitude, lon: item.longitude }) <= 25;
      })
      .slice(0, 50);
  }
  private async nearbyWithFallback(lat: number, lon: number, glutenFree = false) {
    try {
      return await this.nearby(lat, lon, glutenFree);
    } catch {
      if (glutenFree) return this.namedGlutenFreeNearby(lat, lon);
      return this.nearbyFromPhoton(lat, lon);
    }
  }
  async search(params: RestaurantSearch): Promise<Restaurant[]> {
    const key = `search:${params.query.toLocaleLowerCase()}:${params.latitude ?? ''}:${params.longitude ?? ''}:${params.glutenFree ?? false}`;
    const old = this.cached<Restaurant[]>(key);
    if (old) return old;
    if (
      params.latitude !== undefined &&
      params.longitude !== undefined &&
      /^(restaurantes?|cerca de m[ií])$/i.test(params.query)
    )
      return this.store(
        key,
        await this.nearbyWithFallback(params.latitude, params.longitude, params.glutenFree),
      );
    if (params.latitude === undefined && params.longitude === undefined) {
      const locations = await this.photon(params.query, undefined, 5);
      const location = locations.find(
        (f) =>
          f.properties.name?.toLocaleLowerCase() === params.query.toLocaleLowerCase() &&
          !(f.properties.osm_key === 'amenity' && foodType.test(f.properties.osm_value ?? '')),
      );
      if (location)
        return this.store(
          key,
          await this.nearbyWithFallback(
            location.geometry.coordinates[1],
            location.geometry.coordinates[0],
            params.glutenFree,
          ),
        );
    }
    const places = (
      await this.photon(
        params.query,
        undefined,
        20,
        params.latitude !== undefined && params.longitude !== undefined
          ? { lat: params.latitude, lon: params.longitude }
          : undefined,
      )
    )
      .map(fromFeature)
      .filter((p): p is Restaurant => !!p);
    if (places.length) return this.store(key, places);
    const locations = await this.photon(params.query, undefined, 5);
    const location = locations.find(
      (f) => f.properties.name?.toLocaleLowerCase() === params.query.toLocaleLowerCase(),
    );
    return this.store(
      key,
      location
        ? await this.nearbyWithFallback(
            location.geometry.coordinates[1],
            location.geometry.coordinates[0],
            params.glutenFree,
          )
        : [],
    );
  }
  async getById(id: string): Promise<Restaurant | null> {
    const match = /^osm_([nwr])_(\d+)$/.exec(id);
    if (!match) return null;
    this.budget.take();
    const type = codeType[match[1] as keyof typeof codeType];
    const raw = await providerJson(
      `https://api.openstreetmap.org/api/0.6/${type}/${match[2]}.json`,
      { headers: { 'User-Agent': this.userAgent } },
    );
    if (raw === null) return null;
    const item = elementEnvelope.parse(raw).elements[0];
    if (!item?.tags?.name || !isFoodTags(item.tags)) return null;
    return restaurant({
      type: item.type,
      id: item.id,
      name: item.tags.name,
      address: [item.tags['addr:street'], item.tags['addr:housenumber'], item.tags['addr:city']]
        .filter(Boolean)
        .join(', '),
      lat: item.lat,
      lon: item.lon,
      tags: item.tags,
    });
  }
}
