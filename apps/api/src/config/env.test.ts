import { it, expect } from 'vitest';
import { loadEnv } from './env';
it('rejects missing and insecure production configuration', () => {
  expect(() => loadEnv({})).toThrow();
  expect(() =>
    loadEnv({
      NODE_ENV: 'production',
      DATABASE_URL: 'postgres://db',
      JWT_ACCESS_SECRET: 'a'.repeat(64),
      JWT_REFRESH_SECRET: 'b'.repeat(64),
      PUBLIC_APP_URL: 'https://example.org',
      API_PUBLIC_URL: 'https://example.org',
    }),
  ).toThrow('Insecure');
});
