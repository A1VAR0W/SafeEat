import { beforeAll, afterAll, describe, it, expect } from 'vitest';
import { randomUUID } from 'node:crypto';
import { resolve } from 'node:path';
import type { FastifyInstance } from 'fastify';
import { createApp } from '../src/app';
import { createPool } from '../src/db/pool';
import { migrate } from '../src/db/migrate';
import { loadEnv } from '../src/config/env';
import type { ExternalProduct, RestaurantProvider } from '../src/providers/types';
import type { Session } from '@safeeat/contracts';
const url = process.env.TEST_DATABASE_URL;
if (!url)
  throw new Error('TEST_DATABASE_URL is required: use an isolated PostgreSQL test database');
const db = createPool(url);
let app: FastifyInstance;
let one: Session;
let two: Session;
const suffix = randomUUID();
const external: ExternalProduct = {
  barcode: '8410000000001',
  name: 'Integration oats',
  ingredients: 'avena sin gluten',
  gluten: { ingredients: 'avena sin gluten', source: 'Integration test', origin: 'provider' },
  nutrition: {},
  source: 'Integration fixture',
  sourceUrl: 'https://example.org/product',
};
const restaurants: RestaurantProvider = {
  async search() {
    return [(await this.getById('restaurant-1'))!];
  },
  async getById(id) {
    return {
      id,
      name: 'Integration restaurant',
      address: 'Test address',
      mapsUrl: 'https://maps.google.com',
      source: 'Google Maps',
      glutenSupport: 'UNKNOWN',
      evidence: [],
      attributions: [],
      fetchedAt: new Date().toISOString(),
    };
  },
};
const bearer = (s: Session) => ({ authorization: `Bearer ${s.accessToken}` });
beforeAll(async () => {
  await migrate(db, resolve('apps/api/src/db/migrations'));
  const env = loadEnv({
    NODE_ENV: 'test',
    DATABASE_URL: url,
    JWT_ACCESS_SECRET: 'test-secret-access-'.repeat(4),
    JWT_REFRESH_SECRET: 'test-secret-refresh-'.repeat(4),
  });
  app = await createApp(env, {
    db,
    logging: false,
    products: {
      getByBarcode: async (code) => (code === '00000000' ? null : external),
      search: async () => [external],
    },
    restaurants,
  });
  await app.ready();
});
afterAll(async () => {
  if (app) await app.close();
  await db.query('DELETE FROM users WHERE email LIKE $1', [`%${suffix}@example.org`]);
  await db.end();
});
describe.sequential('real PostgreSQL API', () => {
  it('health and readiness', async () => {
    expect((await app.inject('/health')).statusCode).toBe(200);
    expect((await app.inject('/ready')).statusCode).toBe(200);
  });
  it('registers users with hashed passwords and refresh tokens', async () => {
    const r = await app.inject({
      method: 'POST',
      url: '/api/v1/auth/register',
      payload: { email: `one-${suffix}@example.org`, password: 'a-strong-password-123' },
    });
    expect(r.statusCode).toBe(201);
    one = r.json();
    const s = await app.inject({
      method: 'POST',
      url: '/api/v1/auth/register',
      payload: { email: `two-${suffix}@example.org`, password: 'another-password-456' },
    });
    two = s.json();
    const stored = (await db.query('SELECT password_hash FROM users WHERE id=$1', [one.user.id]))
      .rows[0];
    expect(stored.password_hash).toMatch(/^\$argon2id\$/);
    expect(
      (await db.query('SELECT token_hash FROM refresh_sessions WHERE user_id=$1', [one.user.id]))
        .rows[0].token_hash,
    ).not.toBe(one.refreshToken);
  });
  it('rejects duplicate emails and invalid payloads', async () => {
    expect(
      (
        await app.inject({
          method: 'POST',
          url: '/api/v1/auth/register',
          payload: { email: one.user.email, password: 'a-strong-password-123' },
        })
      ).statusCode,
    ).toBe(409);
    expect(
      (
        await app.inject({
          method: 'POST',
          url: '/api/v1/auth/register',
          payload: { email: 'invalid', password: 'x' },
        })
      ).statusCode,
    ).toBe(400);
  });
  it('logs in and rejects wrong passwords', async () => {
    const ok = await app.inject({
      method: 'POST',
      url: '/api/v1/auth/login',
      payload: { email: one.user.email, password: 'a-strong-password-123' },
    });
    expect(ok.statusCode).toBe(200);
    expect(
      (
        await app.inject({
          method: 'POST',
          url: '/api/v1/auth/login',
          payload: { email: one.user.email, password: 'incorrect' },
        })
      ).statusCode,
    ).toBe(401);
  });
  it('protects routes and verifies current user', async () => {
    expect((await app.inject('/api/v1/auth/me')).statusCode).toBe(401);
    expect((await app.inject({ url: '/api/v1/auth/me', headers: bearer(one) })).json()).toEqual(
      one.user,
    );
  });
  it('normalizes products and separates missing product from outages', async () => {
    const r = await app.inject('/api/v1/products/barcode/8410000000001');
    expect(r.statusCode).toBe(200);
    expect(r.json().assessment.status).toBe('GLUTEN_FREE');
    expect((await app.inject('/api/v1/products/barcode/00000000')).statusCode).toBe(404);
  });
  it('adds, deduplicates, isolates and removes favorites', async () => {
    const options = {
      method: 'POST' as const,
      url: '/api/v1/favorites/products/off-8410000000001',
      headers: bearer(one),
    };
    expect((await app.inject(options)).statusCode).toBe(204);
    expect((await app.inject(options)).statusCode).toBe(204);
    expect(
      (await app.inject({ url: '/api/v1/favorites/products', headers: bearer(one) })).json().items,
    ).toHaveLength(1);
    expect(
      (await app.inject({ url: '/api/v1/favorites/products', headers: bearer(two) })).json().items,
    ).toHaveLength(0);
    expect((await app.inject({ ...options, method: 'DELETE' })).statusCode).toBe(204);
  });
  it('favorites restaurant IDs only', async () => {
    expect(
      (
        await app.inject({
          method: 'POST',
          url: '/api/v1/favorites/restaurants/restaurant-1',
          headers: bearer(one),
        })
      ).statusCode,
    ).toBe(204);
    expect(
      (await app.inject({ url: '/api/v1/favorites/restaurants', headers: bearer(one) })).json()
        .items,
    ).toEqual([{ id: 'restaurant-1' }]);
    expect(
      (
        await app.inject({
          method: 'DELETE',
          url: '/api/v1/favorites/restaurants/restaurant-1',
          headers: bearer(one),
        })
      ).statusCode,
    ).toBe(204);
  });
  it('protects private OCR data and shares only an explicit public contract', async () => {
    const created = await app.inject({
      method: 'POST',
      url: '/api/v1/products/analyze-text',
      headers: bearer(one),
      payload: { name: 'My label', text: 'Puede contener trigo', reviewed: true },
    });
    expect(created.statusCode).toBe(201);
    const id = created.json().id;
    expect(
      (await app.inject({ url: `/api/v1/products/${id}`, headers: bearer(two) })).statusCode,
    ).toBe(404);
    expect(
      (
        await app.inject({
          method: 'POST',
          url: `/api/v1/favorites/products/${id}`,
          headers: bearer(two),
        })
      ).statusCode,
    ).toBe(404);
    expect(
      (
        await app.inject({
          method: 'POST',
          url: `/api/v1/shares/products/${id}`,
          headers: bearer(two),
        })
      ).statusCode,
    ).toBe(404);
    const share = await app.inject({
      method: 'POST',
      url: `/api/v1/shares/products/${id}`,
      headers: bearer(one),
    });
    expect(share.statusCode).toBe(201);
    const read = await app.inject(`/api/v1/public/shares/${share.json().token}`);
    expect(read.statusCode).toBe(200);
    expect(read.json().item.name).toBe('My label');
    expect(read.body).not.toContain(one.user.email);
    expect(read.body).not.toContain(one.user.id);
    expect(read.body).not.toContain('password');
    expect((await app.inject(`/api/v1/public/shares/${'f'.repeat(48)}`)).statusCode).toBe(404);
    expect((await app.inject('/api/v1/public/shares/invalid')).statusCode).toBe(400);
  });
  it('rotates refresh credentials and revokes the family on reuse', async () => {
    const r = await app.inject({
      method: 'POST',
      url: '/api/v1/auth/refresh',
      payload: { refreshToken: two.refreshToken },
    });
    expect(r.statusCode).toBe(200);
    const rotated: Session = r.json();
    expect(rotated.refreshToken).not.toBe(two.refreshToken);
    expect(
      (
        await app.inject({
          method: 'POST',
          url: '/api/v1/auth/refresh',
          payload: { refreshToken: two.refreshToken },
        })
      ).statusCode,
    ).toBe(401);
    expect(
      (await app.inject({ url: '/api/v1/auth/me', headers: bearer(rotated) })).statusCode,
    ).toBe(401);
  });
  it('logout revokes access immediately', async () => {
    expect(
      (
        await app.inject({
          method: 'POST',
          url: '/api/v1/auth/logout',
          payload: { refreshToken: one.refreshToken },
        })
      ).statusCode,
    ).toBe(204);
    expect((await app.inject({ url: '/api/v1/auth/me', headers: bearer(one) })).statusCode).toBe(
      401,
    );
  });
  it('deletes accounts and associated shares', async () => {
    const login = await app.inject({
      method: 'POST',
      url: '/api/v1/auth/login',
      payload: { email: one.user.email, password: 'a-strong-password-123' },
    });
    const s: Session = login.json();
    expect(
      (await app.inject({ method: 'DELETE', url: '/api/v1/auth/me', headers: bearer(s) }))
        .statusCode,
    ).toBe(204);
    expect((await db.query('SELECT 1 FROM shares WHERE owner_id=$1', [one.user.id])).rowCount).toBe(
      0,
    );
  });
});
