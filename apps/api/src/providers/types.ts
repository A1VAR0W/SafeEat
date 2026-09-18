import type { NutritionInput, Restaurant } from '@safeeat/contracts';
import type { GlutenInput } from '@safeeat/gluten-engine';
export type ExternalProduct = {
  summaryOnly?: boolean;
  barcode: string;
  name: string;
  brand?: string;
  imageUrl?: string;
  informationSource?: 'api' | 'community' | 'unknown';
  ingredients?: string;
  gluten: GlutenInput;
  nutrition: NutritionInput;
  nutriScore?: 'A' | 'B' | 'C' | 'D' | 'E';
  source: string;
  sourceUrl: string;
};
export interface ProductProvider {
  getByBarcode(barcode: string): Promise<ExternalProduct | null>;
  search(query: string): Promise<ExternalProduct[]>;
}
export type RestaurantSearch = {
  query: string;
  latitude?: number;
  longitude?: number;
  glutenFree?: boolean;
};
export type RestaurantSuggestion = {
  label: string;
  kind: 'restaurant' | 'place';
  query: string;
  latitude?: number;
  longitude?: number;
};
export interface RestaurantProvider {
  search(params: RestaurantSearch): Promise<Restaurant[]>;
  getById(id: string): Promise<Restaurant | null>;
  suggest?(query: string): Promise<RestaurantSuggestion[]>;
}
