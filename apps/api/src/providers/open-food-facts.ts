import { z } from 'zod';
import type { ExternalProduct, ProductProvider } from './types';
import { providerJson, ProviderBudget, safeHttpUrl } from './http';
import { ApiError } from '../plugins/errors';
const productSchema = z.object({
  code: z.union([z.string(), z.number()]).transform(String),
  product_name: z.string().optional(),
  product_name_es: z.string().optional(),
  brands: z.string().optional(),
  ingredients_text: z.string().optional(),
  ingredients_text_es: z.string().optional(),
  allergens_tags: z.array(z.string()).optional(),
  traces_tags: z.array(z.string()).optional(),
  labels_tags: z.array(z.string()).optional(),
  image_front_small_url: z.string().optional(),
  nutriments: z.record(z.string(), z.unknown()).optional(),
  nutrition_grades: z.string().optional(),
});
const fields =
  'code,product_name,product_name_es,brands,ingredients_text,ingredients_text_es,allergens_tags,traces_tags,labels_tags,image_front_small_url,nutriments,nutrition_grades';
export function normalizeOff(value: unknown): ExternalProduct | null {
  const result = productSchema.safeParse(value);
  if (!result.success) return null;
  const p = result.data;
  if (!/^\d{8,14}$/.test(p.code)) return null;
  const sourceUrl = `https://world.openfoodfacts.org/product/${p.code}`;
  const ingredients = p.ingredients_text_es || p.ingredients_text;
  const n = p.nutriments ?? {};
  const number = (key: string, max: number) =>
    typeof n[key] === 'number' && Number.isFinite(n[key]) && n[key] >= 0 && n[key] <= max
      ? n[key]
      : undefined;
  const grade = p.nutrition_grades?.toUpperCase();
  return {
    barcode: p.code,
    name: p.product_name_es || p.product_name || `Producto ${p.code}`,
    brand: p.brands,
    imageUrl: safeHttpUrl(p.image_front_small_url),
    ingredients,
    gluten: {
      ingredients,
      allergens: p.allergens_tags?.join(', '),
      traces: p.traces_tags?.join(', '),
      declaration: p.labels_tags?.some((t) =>
        ['en:gluten-free', 'en:no-gluten', 'es:sin-gluten'].includes(t),
      )
        ? 'sin gluten'
        : undefined,
      source: 'Open Food Facts (datos comunitarios)',
      sourceUrl,
      origin: 'provider',
    },
    nutrition: {
      energyKcal: number('energy-kcal_100g', 900),
      sugars: number('sugars_100g', 100),
      saturatedFat: number('saturated-fat_100g', 100),
      salt:
        number('salt_100g', 100) ??
        (number('sodium_100g', 40) !== undefined ? number('sodium_100g', 40)! * 2.5 : undefined),
      fibre: number('fiber_100g', 100),
      protein: number('proteins_100g', 100),
    },
    nutriScore:
      grade && /^[ABCDE]$/.test(grade) ? (grade as ExternalProduct['nutriScore']) : undefined,
    source: 'Open Food Facts · ODbL',
    sourceUrl,
  };
}
export class OpenFoodFactsProvider implements ProductProvider {
  private reads = new ProviderBudget(14);
  private searches = new ProviderBudget(8);
  private searchCache = new Map<string, { at: number; items: ExternalProduct[] }>();
  constructor(
    private userAgent: string,
    private baseUrl = 'https://world.openfoodfacts.org',
    private searchUrl = 'https://search.openfoodfacts.org',
  ) {}
  async getByBarcode(barcode: string) {
    this.reads.take();
    const raw = await providerJson(`${this.baseUrl}/api/v3/product/${barcode}?fields=${fields}`, {
      headers: { 'User-Agent': this.userAgent },
    });
    if (raw === null) return null;
    const envelope = z
      .object({
        product: z.unknown().optional(),
        status: z.union([z.number(), z.string()]).optional(),
      })
      .safeParse(raw);
    if (!envelope.success)
      throw new ApiError(503, 'PROVIDER_INVALID', 'La fuente ha enviado información no válida.');
    if (envelope.data.status === 0 || envelope.data.status === 'failure') return null;
    const result = normalizeOff(envelope.data.product);
    if (!result)
      throw new ApiError(503, 'PROVIDER_INVALID', 'La fuente ha enviado información incompleta.');
    return result;
  }
  async search(query: string) {
    const key = query.toLocaleLowerCase();
    const cached = this.searchCache.get(key);
    if (cached && Date.now() - cached.at < 300000) return cached.items;
    this.searches.take();
    // The search index supplies discovery summaries, never current safety evidence.
    const params = new URLSearchParams({
      q: query,
      langs: 'es,en',
      page_size: '20',
      fields: 'code,product_name,product_name_es,brands,image_front_small_url',
    });
    const raw = await providerJson(`${this.searchUrl}/search?${params}`, {
      headers: { 'User-Agent': this.userAgent },
    });
    const result = z.object({ hits: z.array(z.unknown()) }).safeParse(raw);
    if (!result.success)
      throw new ApiError(503, 'PROVIDER_INVALID', 'La fuente ha enviado información no válida.');
    const items = result.data.hits
      .map(normalizeSearchHit)
      .filter((v): v is ExternalProduct => v !== null);
    if (this.searchCache.size >= 100)
      this.searchCache.delete(this.searchCache.keys().next().value!);
    this.searchCache.set(key, { at: Date.now(), items });
    return items;
  }
}
export function normalizeSearchHit(value: unknown): ExternalProduct | null {
  const parsed = z
    .object({
      code: z.union([z.string(), z.number()]),
      product_name: z.string().optional(),
      product_name_es: z.string().optional(),
      brands: z.union([z.string(), z.array(z.string())]).optional(),
      image_front_small_url: z.string().optional(),
    })
    .safeParse(value);
  if (!parsed.success) return null;
  const p = parsed.data;
  const item = normalizeOff({
    ...p,
    brands: Array.isArray(p.brands) ? p.brands.join(', ') : p.brands,
  });
  return item
    ? { ...item, summaryOnly: true, source: 'Open Food Facts · índice de búsqueda · ODbL' }
    : null;
}
