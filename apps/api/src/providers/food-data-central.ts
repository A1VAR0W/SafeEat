import { z } from 'zod';
import { assessGluten } from '@safeeat/gluten-engine';
import type { ExternalProduct, ProductProvider } from './types';
import { providerJson, ProviderBudget } from './http';

const resultSchema = z.object({
  foods: z
    .array(
      z.object({
        fdcId: z.number().int(),
        dataType: z.string().optional(),
        gtinUpc: z.string().optional(),
        description: z.string(),
        brandOwner: z.string().optional(),
        brandName: z.string().optional(),
        ingredients: z.string().optional(),
      }),
    )
    .default([]),
});
const digits = (value: string) => value.replace(/^0+/, '') || '0';
export class FoodDataCentralProvider {
  private budget = new ProviderBudget(12);
  constructor(private key: string) {}
  async getByBarcode(barcode: string): Promise<ExternalProduct | null> {
    this.budget.take();
    const raw = await providerJson(
      `https://api.nal.usda.gov/fdc/v1/foods/search?api_key=${encodeURIComponent(this.key)}`,
      {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ query: barcode, dataType: ['Branded'], pageSize: 20 }),
      },
    );
    const food = resultSchema
      .parse(raw)
      .foods.find((item) => item.gtinUpc && digits(item.gtinUpc) === digits(barcode));
    if (!food) return null;
    const sourceUrl = `https://fdc.nal.usda.gov/food-details/${food.fdcId}/nutrients`;
    return {
      barcode,
      name: food.description,
      brand: food.brandName || food.brandOwner,
      ingredients: food.ingredients,
      informationSource: food.ingredients ? 'api' : 'unknown',
      gluten: {
        ingredients: food.ingredients,
        source: 'USDA FoodData Central',
        sourceUrl,
        origin: 'provider',
      },
      nutrition: {},
      source: 'USDA FoodData Central · CC0',
      sourceUrl,
    };
  }
}
export class EnrichedProductProvider implements ProductProvider {
  constructor(
    private primary: ProductProvider,
    private secondary: FoodDataCentralProvider,
  ) {}
  async getByBarcode(barcode: string) {
    let first: ExternalProduct | null = null;
    try {
      first = await this.primary.getByBarcode(barcode);
    } catch (error) {
      try {
        return await this.secondary.getByBarcode(barcode);
      } catch {
        throw error;
      }
    }
    if (first && assessGluten(first.gluten).status !== 'UNKNOWN') return first;
    try {
      const other = await this.secondary.getByBarcode(barcode);
      if (!other) return first;
      if (first && assessGluten(other.gluten).status === 'UNKNOWN') return first;
      return {
        ...other,
        imageUrl: first?.imageUrl,
        nutrition: first?.nutrition ?? other.nutrition,
      };
    } catch {
      return first;
    }
  }
  search(query: string) {
    return this.primary.search(query);
  }
}
