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
function restaurant(input: {
  type: 'node' | 'way' | 'relation';
  id: number;
  name: string;
  address: string;
  lat?: number;
  lon?: number;
  tags?: Record<string, string>;
}): Restaurant {
  const mapsUrl = `https://www.openstreetmap.org/${input.type}/${input.id}`;
  const glutenTag = input.tags?.['diet:gluten_free'];
  const glutenSupport =
    glutenTag === 'only'
      ? 'DEDICATED_GLUTEN_FREE'
      : glutenTag === 'yes' || glutenTag === 'limited'
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
    evidence: [
      {
        source: 'OpenStreetMap',
        kind: 'uncertainty',
        text:
          glutenSupport === 'DEDICATED_GLUTEN_FREE'
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
  if (!p.name || p.osm_key !== 'amenity') return null;
  return restaurant({
    type: typeCode[p.osm_type],
    id: p.osm_id,
    name: p.name,
    address: [p.street, p.housenumber, p.city, p.state, p.country].filter(Boolean).join(', '),
    lat: f.geometry.coordinates[1],
    lon: f.geometry.coordinates[0],
  });
}
export class OpenStreetMapProvider implements RestaurantProvider {
  private budget = new ProviderBudget(45);
  private cache = new Map<string, { at: number; value: Restaurant[] | RestaurantSuggestion[] }>();
  constructor(private userAgent: string) {}
  private async photon(
    q: string,
    tag: string,
    limit = 10,
    location?: { lat: number; lon: number },
  ) {
    this.budget.take();
    // Photon does not support Spanish as a language code. Omitting `lang`
    // lets the service use its default language while keeping the query
    // compatible with the public endpoint.
    const params = new URLSearchParams({ q, limit: String(limit), osm_tag: tag });
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
  async suggest(query: string): Promise<RestaurantSuggestion[]> {
    const key = `suggest:${query.toLocaleLowerCase()}`;
    const old = this.cached<RestaurantSuggestion[]>(key);
    if (old) return old;
    const [places, cities] = await Promise.all([
      this.photon(query, 'amenity:restaurant', 5),
      this.photon(query, 'place:city', 3),
    ]);
    return this.store(key, [
      ...cities
        .filter((f) => f.properties.name)
        .map((f) => ({
          label: [f.properties.name, f.properties.country].filter(Boolean).join(' · '),
          kind: 'city' as const,
          query: 'restaurantes',
          latitude: f.geometry.coordinates[1],
          longitude: f.geometry.coordinates[0],
        })),
      ...places
        .filter((f) => f.properties.name)
        .map((f) => ({
          label: [f.properties.name, f.properties.city].filter(Boolean).join(' · '),
          kind: 'restaurant' as const,
          query: [f.properties.name, f.properties.city].filter(Boolean).join(' '),
        })),
    ]);
  }
  private async nearby(lat: number, lon: number, glutenFree = false) {
    this.budget.take();
    const tag = glutenFree ? '["diet:gluten_free"~"^(yes|only|limited)$"]' : '';
    const q = `[out:json][timeout:8];nwr[amenity=restaurant]${tag}(around:${glutenFree ? 3000 : 1200},${lat},${lon});out center ${glutenFree ? 80 : 35};`;
    const data = elementEnvelope.parse(
      await providerJson(`https://overpass-api.de/api/interpreter?data=${encodeURIComponent(q)}`, {
        headers: { 'User-Agent': this.userAgent },
      }),
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
        await this.nearby(params.latitude, params.longitude, params.glutenFree),
      );
    if (params.latitude === undefined && params.longitude === undefined) {
      const cities = await this.photon(params.query, 'place:city', 2);
      const city = cities.find(
        (f) => f.properties.name?.toLocaleLowerCase() === params.query.toLocaleLowerCase(),
      );
      if (city)
        return this.store(
          key,
          await this.nearby(
            city.geometry.coordinates[1],
            city.geometry.coordinates[0],
            params.glutenFree,
          ),
        );
    }
    const places = (
      await this.photon(
        params.query,
        'amenity:restaurant',
        20,
        params.latitude !== undefined && params.longitude !== undefined
          ? { lat: params.latitude, lon: params.longitude }
          : undefined,
      )
    )
      .map(fromFeature)
      .filter((p): p is Restaurant => !!p);
    if (places.length) return this.store(key, places);
    const cities = await this.photon(params.query, 'place:city', 2);
    const city = cities.find(
      (f) => f.properties.name?.toLocaleLowerCase() === params.query.toLocaleLowerCase(),
    );
    return this.store(
      key,
      city
        ? await this.nearby(
            city.geometry.coordinates[1],
            city.geometry.coordinates[0],
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
    if (!item?.tags?.name || item.tags.amenity !== 'restaurant') return null;
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
