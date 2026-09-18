import { afterEach, describe, expect, it, vi } from 'vitest';
import { FoodDataCentralProvider, EnrichedProductProvider } from './food-data-central';
import { OpenStreetMapProvider } from './openstreetmap';
import type { ProductProvider } from './types';

afterEach(() => vi.unstubAllGlobals());
describe('additional real-data sources', () => {
  it('only accepts an exact branded GTIN from USDA and improves missing ingredient evidence', async () => {
    const request = vi.fn(
      async () =>
        new Response(
          JSON.stringify({
            foods: [
              { fdcId: 1, gtinUpc: '12345678', description: 'Wrong item', ingredients: 'WHEAT' },
              {
                fdcId: 2,
                gtinUpc: '008410000000001',
                description: 'Matched item',
                ingredients: 'WHEAT FLOUR',
              },
            ],
          }),
          { status: 200 },
        ),
    );
    vi.stubGlobal('fetch', request);
    const fallback = new FoodDataCentralProvider('test-key');
    const primary: ProductProvider = {
      getByBarcode: async () => ({
        barcode: '8410000000001',
        name: 'Incomplete',
        nutrition: {},
        source: 'OFF',
        sourceUrl: 'https://example.org',
        gluten: { source: 'OFF' },
      }),
      search: async () => [],
    };
    const item = await new EnrichedProductProvider(primary, fallback).getByBarcode('8410000000001');
    expect(item?.ingredients).toBe('WHEAT FLOUR');
    expect(item?.source).toContain('USDA');
    expect(String(request.mock.calls[0])).toContain('api.nal.usda.gov');
  });
  it('normalizes real restaurant records while keeping gluten support unknown', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(
        async () =>
          new Response(
            JSON.stringify({
              features: [
                {
                  properties: {
                    osm_type: 'N',
                    osm_id: 3265474259,
                    osm_key: 'amenity',
                    osm_value: 'restaurant',
                    name: 'La Tagliatella',
                    city: 'Madrid',
                    country: 'España',
                  },
                  geometry: { coordinates: [-3.6733, 40.4393] },
                },
              ],
            }),
            { status: 200 },
          ),
      ),
    );
    const item = (
      await new OpenStreetMapProvider('SafeEat test').search({ query: 'La Tagliatella Madrid' })
    )[0];
    expect(item?.id).toBe('osm_n_3265474259');
    expect(item?.name).toBe('La Tagliatella');
    expect(item?.glutenSupport).toBe('UNKNOWN');
    expect(item?.mapsUrl).toContain('openstreetmap.org/node/3265474259');
  });
  it('resolves a city to nearby restaurants with map coordinates', async () => {
    const request = vi.fn(
      async (url: string, _options?: RequestInit) =>
        new Response(
          JSON.stringify(
            url.includes('photon.komoot.io')
              ? {
                  features: [
                    {
                      properties: { osm_type: 'R', osm_id: 11, name: 'Madrid', osm_key: 'place' },
                      geometry: { coordinates: [-3.7038, 40.4168] },
                    },
                  ],
                }
              : {
                  elements: [
                    {
                      type: 'way',
                      id: 42,
                      center: { lat: 40.417, lon: -3.704 },
                      tags: {
                        amenity: 'restaurant',
                        name: 'Restaurante local',
                        'diet:gluten_free': 'yes',
                      },
                    },
                  ],
                },
          ),
          { status: 200 },
        ),
    );
    vi.stubGlobal('fetch', request);
    const provider = new OpenStreetMapProvider('SafeEat test');
    const items = await provider.search({ query: 'Madrid' });
    expect(items).toMatchObject([
      {
        id: 'osm_w_42',
        name: 'Restaurante local',
        latitude: 40.417,
        longitude: -3.704,
        glutenSupport: 'GLUTEN_FREE_OPTIONS',
      },
    ]);
    expect(request.mock.calls.map(([url]) => url)).toEqual([
      expect.stringContaining('photon.komoot.io'),
      expect.stringContaining('overpass-api.de'),
    ]);
    expect((await provider.search({ query: 'Madrid', glutenFree: true }))[0]?.glutenSupport).toBe(
      'GLUTEN_FREE_OPTIONS',
    );
    expect(request.mock.calls[3]?.[1]?.body).toContain('diet%3Agluten_free');
  });
  it('uses the visible map area for street suggestions and removes duplicate segments', async () => {
    const request = vi.fn(
      async (_url: string) =>
        new Response(
          JSON.stringify({
            features: [
              {
                properties: {
                  osm_type: 'W',
                  osm_id: 1,
                  name: 'Gran Vía',
                  city: 'Madrid',
                  osm_key: 'highway',
                  osm_value: 'tertiary',
                },
                geometry: { coordinates: [-3.704, 40.42] },
              },
              {
                properties: {
                  osm_type: 'W',
                  osm_id: 2,
                  name: 'Gran Vía',
                  city: 'Madrid',
                  osm_key: 'highway',
                  osm_value: 'tertiary',
                },
                geometry: { coordinates: [-3.71, 40.423] },
              },
            ],
          }),
          { status: 200 },
        ),
    );
    vi.stubGlobal('fetch', request);
    const suggestions = await new OpenStreetMapProvider('SafeEat test').suggest('Gran Vía', {
      latitude: 40.4168,
      longitude: -3.7038,
    });
    expect(suggestions).toMatchObject([
      { label: 'Gran Vía · Madrid', kind: 'place', query: 'restaurantes' },
    ]);
    expect(request.mock.calls[0]?.[0]).toContain('lat=40.42');
    expect(request.mock.calls[0]?.[0]).toContain('lon=-3.7');
  });
});
