import { z } from 'zod';
import type { Restaurant } from '@safeeat/contracts';
import type { RestaurantProvider, RestaurantSearch, RestaurantSuggestion } from './types';
import { providerJson, safeHttpUrl, ProviderBudget } from './http';
import { ApiError } from '../plugins/errors';
const place = z.object({
  id: z.string(),
  displayName: z.object({ text: z.string() }),
  formattedAddress: z.string().optional(),
  location: z.object({ latitude: z.number(), longitude: z.number() }).optional(),
  googleMapsUri: z.string().optional(),
  websiteUri: z.string().optional(),
  nationalPhoneNumber: z.string().optional(),
  attributions: z
    .array(z.object({ provider: z.string().optional(), providerUri: z.string().optional() }))
    .optional(),
});
const fields =
  'id,displayName,formattedAddress,location,googleMapsUri,websiteUri,nationalPhoneNumber,attributions';
function normalize(value: unknown): Restaurant {
  const p = place.parse(value);
  return {
    id: p.id,
    name: p.displayName.text,
    address: p.formattedAddress ?? '',
    latitude: p.location?.latitude,
    longitude: p.location?.longitude,
    website: safeHttpUrl(p.websiteUri),
    phone: p.nationalPhoneNumber,
    mapsUrl:
      safeHttpUrl(p.googleMapsUri) ??
      `https://www.google.com/maps/search/?api=1&query=${encodeURIComponent(p.displayName.text)}&query_place_id=${encodeURIComponent(p.id)}`,
    glutenSupport: 'UNKNOWN',
    evidence: [
      {
        source: 'Google Maps',
        kind: 'uncertainty',
        text: 'Esta fuente no verifica protocolos sin gluten ni contaminación cruzada.',
      },
    ],
    source: 'Google Maps',
    attributions: (p.attributions ?? []).map((a) => a.provider ?? '').filter(Boolean),
    fetchedAt: new Date().toISOString(),
  };
}
export class GooglePlacesProvider implements RestaurantProvider {
  private budget = new ProviderBudget(60);
  constructor(private key?: string) {}
  private headers(mask: string) {
    if (!this.key)
      throw new ApiError(
        503,
        'RESTAURANT_PROVIDER_NOT_CONFIGURED',
        'La búsqueda de restaurantes todavía no está disponible. Falta configurar el proveedor.',
      );
    this.budget.take();
    return {
      'Content-Type': 'application/json',
      'X-Goog-Api-Key': this.key,
      'X-Goog-FieldMask': mask,
    };
  }
  async search(params: RestaurantSearch) {
    const body = {
      textQuery: params.query,
      includedType: 'restaurant',
      strictTypeFiltering: true,
      languageCode: 'es',
      pageSize: 20,
      ...(params.latitude !== undefined && params.longitude !== undefined
        ? {
            locationBias: {
              circle: {
                center: { latitude: params.latitude, longitude: params.longitude },
                radius: 5000,
              },
            },
          }
        : {}),
    };
    const result = await providerJson('https://places.googleapis.com/v1/places:searchText', {
      method: 'POST',
      headers: this.headers(
        fields
          .split(',')
          .map((f) => `places.${f}`)
          .join(','),
      ),
      body: JSON.stringify(body),
    });
    const parsed = z.object({ places: z.array(z.unknown()).default([]) }).parse(result);
    return parsed.places.map(normalize);
  }
  async suggest(query: string): Promise<RestaurantSuggestion[]> {
    const result = await providerJson('https://places.googleapis.com/v1/places:autocomplete', {
      method: 'POST',
      headers: this.headers(
        'suggestions.placePrediction.text,suggestions.placePrediction.types,suggestions.queryPrediction.text',
      ),
      body: JSON.stringify({ input: query, languageCode: 'es', includeQueryPredictions: true }),
    });
    const parsed = z
      .object({
        suggestions: z
          .array(
            z.object({
              placePrediction: z
                .object({
                  text: z.object({ text: z.string() }),
                  types: z.array(z.string()).optional(),
                })
                .optional(),
              queryPrediction: z.object({ text: z.object({ text: z.string() }) }).optional(),
            }),
          )
          .default([]),
      })
      .parse(result);
    return parsed.suggestions.flatMap((s) => {
      const label = s.placePrediction?.text.text ?? s.queryPrediction?.text.text;
      if (!label) return [];
      return [
        {
          label,
          query: label,
          kind: s.placePrediction?.types?.some((t) =>
            ['locality', 'administrative_area_level_1', 'administrative_area_level_2'].includes(t),
          )
            ? ('city' as const)
            : ('restaurant' as const),
        },
      ];
    });
  }
  async getById(id: string) {
    const raw = await providerJson(
      `https://places.googleapis.com/v1/places/${encodeURIComponent(id)}?languageCode=es`,
      { headers: this.headers(fields) },
    );
    return raw === null ? null : normalize(raw);
  }
}
