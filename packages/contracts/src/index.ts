import { z } from 'zod';

export const GlutenStatusSchema = z.enum([
  'GLUTEN_FREE',
  'MAY_CONTAIN_TRACES',
  'CONTAINS_GLUTEN',
  'UNKNOWN',
]);
export type GlutenStatus = z.infer<typeof GlutenStatusSchema>;
export const EvidenceSchema = z.object({
  source: z.string(),
  text: z.string(),
  kind: z.enum(['claim', 'ingredient', 'allergen', 'trace', 'uncertainty']),
  url: z.string().url().optional(),
});
export type EvidenceItem = z.infer<typeof EvidenceSchema>;
export const AssessmentSchema = z.object({
  status: GlutenStatusSchema,
  confidence: z.number().min(0).max(1),
  reasons: z.array(z.object({ code: z.string(), message: z.string() })),
  evidence: z.array(EvidenceSchema),
  engineVersion: z.string(),
});
export type GlutenAssessment = z.infer<typeof AssessmentSchema>;
export const NutritionInputSchema = z.object({
  energyKcal: z.number().finite().min(0).max(900).optional(),
  saturatedFat: z.number().finite().min(0).max(100).optional(),
  sugars: z.number().finite().min(0).max(100).optional(),
  salt: z.number().finite().min(0).max(100).optional(),
  fibre: z.number().finite().min(0).max(100).optional(),
  protein: z.number().finite().min(0).max(100).optional(),
});
export type NutritionInput = z.infer<typeof NutritionInputSchema>;
export const NutritionScoreSchema = z.object({
  score: z.number().int().min(0).max(100).nullable(),
  breakdown: z.array(z.object({ label: z.string(), points: z.number() })),
  explanation: z.string(),
  version: z.string(),
});
export type NutritionScore = z.infer<typeof NutritionScoreSchema>;
export const ProductSchema = z.object({
  id: z.string(),
  barcode: z.string().optional(),
  name: z.string(),
  brand: z.string().optional(),
  imageUrl: z.string().url().optional(),
  ingredientsImageUrl: z.string().url().optional(),
  informationSource: z.enum(['api', 'community', 'unknown']).default('unknown'),
  communityContribution: z.boolean().default(false),
  glutenReports: z
    .object({ glutenFree: z.number().int().min(0), containsGluten: z.number().int().min(0) })
    .optional(),
  verifiedGlutenReports: z
    .object({ glutenFree: z.number().int().min(0), containsGluten: z.number().int().min(0) })
    .optional(),
  ingredients: z.string().optional(),
  assessment: AssessmentSchema,
  nutrition: NutritionInputSchema,
  quality: NutritionScoreSchema,
  nutriScore: z.enum(['A', 'B', 'C', 'D', 'E']).optional(),
  source: z.string(),
  sourceUrl: z.string().url().optional(),
  fetchedAt: z.string(),
  stale: z.boolean().default(false),
  userCreated: z.boolean().default(false),
  /** Label text contributed by a user; shown next to the provider's list, never instead of it. */
  communityIngredients: z.string().optional(),
  /** The provider's own analysis when a community label raised the risk in `assessment`. */
  providerAssessment: AssessmentSchema.optional(),
});
export type Product = z.infer<typeof ProductSchema>;
export const CommunityReviewSchema = z.object({
  id: z.string().uuid(),
  revision: z.string().uuid(),
  own: z.boolean(),
  confirmedByMe: z.boolean(),
  note: z.string(),
  status: z.string(),
  updatedAt: z.string(),
  verifiedAt: z.string().nullable(),
  verificationMethod: z.enum(['manual', 'authorized']).nullable().default(null),
  confirmations: z.number().int(),
  inherited: z.boolean(),
});
export type CommunityReview = z.infer<typeof CommunityReviewSchema>;
export const RestaurantSchema = z.object({
  id: z.string(),
  name: z.string(),
  address: z.string(),
  latitude: z.number().optional(),
  longitude: z.number().optional(),
  website: z.string().url().optional(),
  phone: z.string().optional(),
  mapsUrl: z.string().url(),
  glutenSupport: z.enum([
    'DEDICATED_GLUTEN_FREE',
    'GLUTEN_FREE_OPTIONS',
    'NO_GLUTEN_FREE_OPTIONS',
    'UNKNOWN',
  ]),
  communityCreated: z.boolean().optional(),
  chain: z.object({ key: z.string(), name: z.string() }).optional(),
  reviewGroup: z
    .object({ key: z.string(), name: z.string(), count: z.number().int().min(2) })
    .optional(),
  rating: z
    .object({
      average: z.number().min(1).max(5),
      count: z.number().int().positive(),
    })
    .optional(),
  glutenReports: z
    .object({
      dedicated: z.number().int().min(0),
      options: z.number().int().min(0),
      noOptions: z.number().int().min(0),
    })
    .optional(),
  evidence: z.array(EvidenceSchema),
  source: z.string(),
  attributions: z.array(z.string()).default([]),
  fetchedAt: z.string(),
});
export type Restaurant = z.infer<typeof RestaurantSchema>;
export const credentialsSchema = z
  .object({
    email: z
      .email()
      .max(254)
      .transform((v) => v.trim().toLowerCase()),
    password: z.string().min(12).max(128),
  })
  .strict();
export const loginSchema = z
  .object({
    email: z
      .email()
      .max(254)
      .transform((v) => v.trim().toLowerCase()),
    password: z.string().min(1).max(128),
  })
  .strict();
export const dateOfBirthSchema = z.iso
  .date()
  .refine(
    (value) => value >= '1900-01-01' && value < new Date().toISOString().slice(0, 10),
    'Introduce una fecha de nacimiento válida y anterior a hoy.',
  );
export const profileFieldsSchema = z
  .object({
    firstName: z.string().trim().min(1).max(80),
    lastName: z.string().trim().max(120).optional(),
    dateOfBirth: dateOfBirthSchema,
  })
  .strict();
export const registrationSchema = credentialsSchema.extend({
  ...profileFieldsSchema.shape,
  productUpdatesConsent: z.boolean().default(false),
});
export const emailChangeSchema = z
  .object({ email: credentialsSchema.shape.email, currentPassword: loginSchema.shape.password })
  .strict();
export const passwordChangeSchema = z
  .object({
    currentPassword: loginSchema.shape.password,
    newPassword: credentialsSchema.shape.password,
  })
  .strict();
export const deleteAccountSchema = z
  .object({ currentPassword: loginSchema.shape.password, confirmation: z.literal('ELIMINAR') })
  .strict();
export const refreshSchema = z
  .object({ refreshToken: z.string().regex(/^[a-f0-9]{96}$/) })
  .strict();
export const analyzeSchema = z
  .object({
    name: z.string().trim().min(1).max(160).default('Etiqueta escaneada'),
    text: z.string().trim().min(3).max(12000),
    reviewed: z.boolean().default(false),
  })
  .strict();
export const UserSchema = z.object({
  id: z.string().uuid(),
  email: z.email(),
  roles: z.array(z.enum(['authorized_celiac', 'ADMIN'])).default([]),
  firstName: z.string().nullable().default(null),
  lastName: z.string().nullable().default(null),
  dateOfBirth: z.iso.date().nullable().default(null),
  productUpdatesConsent: z.boolean().default(false),
  productUpdatesConsentAt: z.string().nullable().default(null),
  productUpdatesConsentVersion: z.string().nullable().default(null),
});
export type User = z.infer<typeof UserSchema>;
export const SessionSchema = z.object({
  accessToken: z.string(),
  refreshToken: z.string(),
  user: UserSchema,
});
export type Session = z.infer<typeof SessionSchema>;
export const ShareSchema = z.discriminatedUnion('type', [
  z.object({ type: z.literal('product'), item: ProductSchema }),
  z.object({ type: z.literal('restaurant'), item: RestaurantSchema }),
]);
export type PublicShare = z.infer<typeof ShareSchema>;
export const statusLabels: Record<GlutenStatus, { icon: string; label: string }> = {
  GLUTEN_FREE: { icon: '✓', label: 'SIN GLUTEN DECLARADO' },
  MAY_CONTAIN_TRACES: { icon: '!', label: 'PUEDE CONTENER TRAZAS' },
  CONTAINS_GLUTEN: { icon: '×', label: 'CONTIENE GLUTEN' },
  UNKNOWN: { icon: '⚠', label: 'NO VERIFICADO' },
};
export const restaurantDisclaimer =
  'Confirma siempre con el restaurante sus protocolos y posibles contaminaciones cruzadas.';
export const productDisclaimer =
  'Comprueba la etiqueta actual. La información puede estar incompleta o haber cambiado. La confianza refleja la evidencia disponible, no una probabilidad de seguridad.';
export function parseShareLink(
  value: string,
  publicAppUrl: string,
): { type: 'product' | 'restaurant'; token: string } | null {
  try {
    const url = new URL(value);
    const custom = url.protocol === 'safeeat:';
    const base = new URL(publicAppUrl);
    if (!custom && url.origin !== base.origin) return null;
    const prefix = base.pathname.replace(/\/$/, '');
    if (!custom && !url.pathname.startsWith(`${prefix}/`)) return null;
    const path = custom ? `/${url.hostname}${url.pathname}` : url.pathname.slice(prefix.length);
    const match = /^\/(p|r)\/([a-f0-9]{48})$/.exec(path);
    return match ? { type: match[1] === 'p' ? 'product' : 'restaurant', token: match[2]! } : null;
  } catch {
    return null;
  }
}
