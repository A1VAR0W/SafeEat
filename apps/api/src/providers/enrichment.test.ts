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
});
