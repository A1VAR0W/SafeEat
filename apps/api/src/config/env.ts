import { z } from 'zod';
const schema = z.object({
  NODE_ENV: z.enum(['development', 'test', 'production']).default('development'),
  APP_ENV: z.enum(['development', 'preproduction', 'production']).default('development'),
  PORT: z.coerce.number().int().min(1).max(65535).default(4000),
  DATABASE_URL: z.string().startsWith('postgres'),
  JWT_ACCESS_SECRET: z.string().min(48),
  JWT_REFRESH_SECRET: z.string().min(48),
  PUBLIC_APP_URL: z.url().default('http://localhost:3000'),
  API_PUBLIC_URL: z.url().default('http://localhost:4000'),
  OPENFOODFACTS_USER_AGENT: z
    .string()
    .min(10)
    .default('SafeEat/0.1 (https://github.com/A1VAR0W/SafeEat)'),
  OPENFOODFACTS_BASE_URL: z.url().default('https://world.openfoodfacts.org'),
  GOOGLE_PLACES_API_KEY: z.string().optional(),
  TRUST_PROXY: z.coerce.number().int().min(0).max(2).default(0),
});
export type Env = z.infer<typeof schema>;
export function loadEnv(values: NodeJS.ProcessEnv = process.env): Env {
  const result = schema.safeParse(values);
  if (!result.success)
    throw new Error(
      `Invalid environment variables: ${result.error.issues.map((x) => x.path.join('.')).join(', ')}`,
    );
  const env = result.data;
  if (env.NODE_ENV === 'production') {
    if (![env.API_PUBLIC_URL, env.PUBLIC_APP_URL].every((v) => new URL(v).protocol === 'https:'))
      throw new Error('Production public URLs must use HTTPS');
    if (
      [env.JWT_ACCESS_SECRET, env.JWT_REFRESH_SECRET].some(
        (v) => /example|change|development|test-secret/i.test(v) || new Set(v).size < 16,
      )
    )
      throw new Error('Insecure production secret');
    if (env.JWT_ACCESS_SECRET === env.JWT_REFRESH_SECRET)
      throw new Error('Access and refresh secrets must differ');
    if (env.APP_ENV === 'development')
      throw new Error('Production runtime requires explicit APP_ENV');
  }
  return env;
}
