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
import { ApiError } from '../src/plugins/errors';
import sharp from 'sharp';
const url = process.env.TEST_DATABASE_URL;
if (!url)
  throw new Error('TEST_DATABASE_URL is required: use an isolated PostgreSQL test database');
const db = createPool(url);
let app: FastifyInstance;
let one: Session;
let two: Session;
const suffix = randomUUID();
let providerUnavailable = false;
let suggestionFailure = false;
let suggestionSlow = false;
let suggestionCalls = 0;
let suggestionCancelled = false;
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
  async suggest(_query, _location, signal) {
    suggestionCalls++;
    if (suggestionFailure) throw new ApiError(503, 'PROVIDER_UNAVAILABLE', 'Provider unavailable');
    if (suggestionSlow)
      await new Promise<void>((_resolve, reject) =>
        signal!.addEventListener(
          'abort',
          () => {
            suggestionCancelled = true;
            reject(new Error('cancelled'));
          },
          { once: true },
        ),
      );
    return [
      {
        label: 'Madrid · España',
        kind: 'place',
        query: 'restaurantes',
        latitude: 40.4,
        longitude: -3.7,
      },
    ];
  },
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
      chain: id.startsWith('chain-') ? { key: 'Q999999999', name: 'Test chain' } : undefined,
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
      getByBarcode: async (code) => {
        if (providerUnavailable)
          throw new ApiError(503, 'PROVIDER_UNAVAILABLE', 'Provider unavailable');
        return code === '00000000' ? null : external;
      },
      search: async () =>
        ['8410000000001', '8410000000002'].map((barcode) => ({
          ...external,
          barcode,
          name: 'Index summary',
          summaryOnly: true,
          ingredients: undefined,
          nutrition: {},
          gluten: { source: 'Search index' },
        })),
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
      payload: {
        firstName: 'Integration',
        dateOfBirth: '1990-06-15',
        email: `one-${suffix}@example.org`,
        password: 'a-strong-password-123',
      },
    });
    expect(r.statusCode).toBe(201);
    one = r.json();
    const s = await app.inject({
      method: 'POST',
      url: '/api/v1/auth/register',
      payload: {
        firstName: 'Integration',
        dateOfBirth: '1990-06-15',
        email: `two-${suffix}@example.org`,
        password: 'another-password-456',
      },
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
  it('serves fast local suggestions and preserves them when external suggestions fail or time out', async () => {
    const id = `se_${randomUUID().replace(/-/g, '')}`;
    await db.query(
      'INSERT INTO community_restaurants(id,user_id,name,address,latitude,longitude) VALUES($1,$2,$3,$4,40.4,-3.7)',
      [id, one.user.id, 'Provider Cafe', 'Integration test address'],
    );
    try {
      const calls = suggestionCalls;
      const local = await app.inject('/api/v1/restaurants/suggest?q=Provider&localOnly=true');
      expect(local.statusCode).toBe(200);
      expect(local.json().items.some((item: { id: string }) => item.id === id)).toBe(true);
      expect(suggestionCalls).toBe(calls);
      suggestionFailure = true;
      const fallback = await app.inject('/api/v1/restaurants/suggest?q=Provider');
      expect(fallback.statusCode).toBe(200);
      expect(fallback.json().items.some((item: { id: string }) => item.id === id)).toBe(true);
      suggestionFailure = false;
      suggestionSlow = true;
      const timed = await app.inject('/api/v1/restaurants/suggest?q=Provider');
      expect(timed.statusCode).toBe(200);
      expect(timed.json().items.some((item: { id: string }) => item.id === id)).toBe(true);
      expect(suggestionCancelled).toBe(true);
    } finally {
      suggestionFailure = false;
      suggestionSlow = false;
      await db.query('DELETE FROM community_restaurants WHERE id=$1', [id]);
    }
  });
  it('rejects duplicate emails and invalid payloads', async () => {
    expect(
      (
        await app.inject({
          method: 'POST',
          url: '/api/v1/auth/register',
          payload: {
            firstName: 'Integration',
            dateOfBirth: '1990-06-15',
            email: one.user.email,
            password: 'a-strong-password-123',
          },
        })
      ).statusCode,
    ).toBe(409);
    expect(
      (
        await app.inject({
          method: 'POST',
          url: '/api/v1/auth/register',
          payload: {
            firstName: 'Integration',
            dateOfBirth: '1990-06-15',
            email: 'invalid',
            password: 'x',
          },
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
  it('search summaries never overwrite full product evidence and require detail refresh', async () => {
    const response = await app.inject('/api/v1/products/search?q=arroz');
    expect(response.statusCode).toBe(200);
    const [full, summary] = response.json().items;
    expect(full.name).toBe('Integration oats');
    expect(full.assessment.status).toBe('GLUTEN_FREE');
    expect(summary.assessment.status).toBe('UNKNOWN');
    expect(summary.stale).toBe(true);
    expect(summary.quality.score).toBeNull();
    const row = (
      await db.query(
        "SELECT expires_at<now() AS expired FROM products WHERE id='off-8410000000002'",
      )
    ).rows[0];
    expect(row.expired).toBe(true);
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
  it('checks favorite membership beyond the first page and isolates users', async () => {
    const ids = Array.from(
      { length: 51 },
      (_, n) => `page-${suffix}-${String(n).padStart(3, '0')}`,
    );
    const template = (await app.inject('/api/v1/products/barcode/8410000000001')).json();
    for (const id of ids) {
      await db.query(
        "INSERT INTO products(id,provider,data,expires_at) VALUES($1,'test',$2,now()+interval '1 day')",
        [id, { ...template, id }],
      );
      await db.query('INSERT INTO favorite_products(user_id,product_id) VALUES($1,$2)', [
        one.user.id,
        id,
      ]);
    }
    const page = (
      await app.inject({ url: '/api/v1/favorites/products', headers: bearer(one) })
    ).json();
    expect(page.items).toHaveLength(50);
    expect(page.items.some((p: { id: string }) => p.id === ids[50])).toBe(false);
    const url = `/api/v1/favorites/products/${ids[50]}`;
    expect((await app.inject({ url, headers: bearer(one) })).json()).toEqual({ saved: true });
    expect((await app.inject({ url, headers: bearer(two) })).json()).toEqual({ saved: false });
    const next = (
      await app.inject({
        url: `/api/v1/favorites/products?cursor=${encodeURIComponent(page.nextCursor)}`,
        headers: bearer(one),
      })
    ).json();
    expect(next.items.map((p: { id: string }) => p.id)).toEqual([ids[50]]);
    await db.query('DELETE FROM products WHERE id=ANY($1::text[])', [ids]);
  });
  it('returns stale cached products during outages, but 503 for uncached products', async () => {
    await db.query(
      "UPDATE products SET expires_at=now()-interval '1 second' WHERE id='off-8410000000001'",
    );
    providerUnavailable = true;
    try {
      const cached = await app.inject('/api/v1/products/barcode/8410000000001');
      expect(cached.statusCode).toBe(200);
      expect(cached.json().stale).toBe(true);
      const uncached = await app.inject('/api/v1/products/barcode/99999999');
      expect(uncached.statusCode).toBe(503);
      expect(uncached.json().error.code).toBe('PROVIDER_UNAVAILABLE');
    } finally {
      providerUnavailable = false;
    }
  });
  it('rejects expired or tampered JWTs, SQL-shaped IDs and oversized payloads', async () => {
    const expired = app.jwt.sign({ sub: one.user.id, sid: randomUUID() }, { expiresIn: -1 });
    for (const token of [expired, one.accessToken + 'tampered']) {
      expect(
        (
          await app.inject({
            url: '/api/v1/auth/me',
            headers: { authorization: `Bearer ${token}` },
          })
        ).statusCode,
      ).toBe(401);
    }
    expect(
      (
        await app.inject({
          url: `/api/v1/favorites/products/${encodeURIComponent("' OR 1=1 --")}`,
          headers: bearer(one),
        })
      ).json(),
    ).toEqual({ saved: false });
    expect(
      (
        await app.inject({
          method: 'POST',
          url: '/api/v1/products/analyze-text',
          headers: bearer(one),
          payload: { name: 'x', text: 'x'.repeat(40000), reviewed: true },
        })
      ).statusCode,
    ).toBe(413);
    const cors = await app.inject({
      method: 'OPTIONS',
      url: '/api/v1/auth/me',
      headers: { origin: 'https://evil.example', 'access-control-request-method': 'GET' },
    });
    expect(cors.headers['access-control-allow-origin']).not.toBe('https://evil.example');
  });
  it('shares restaurants anonymously with non-enumerable tokens', async () => {
    const share = await app.inject({
      method: 'POST',
      url: '/api/v1/shares/restaurants/restaurant-1',
    });
    expect(share.statusCode).toBe(201);
    expect(share.json().token).toMatch(/^[a-f0-9]{48}$/);
    const publicItem = await app.inject(`/api/v1/public/shares/${share.json().token}`);
    expect(publicItem.statusCode).toBe(200);
    expect(publicItem.json().type).toBe('restaurant');
    expect(publicItem.body).not.toContain(one.user.id);
  });
  it('stores reviewed ingredient photos and shares product evidence by barcode', async () => {
    const photo = await sharp({
      create: { width: 20, height: 20, channels: 3, background: '#ffffff' },
    })
      .jpeg()
      .toBuffer();
    const url = '/api/v1/products/00000000/ingredients-photo';
    const payload = {
      name: 'Producto de prueba',
      ingredients: 'Harina de trigo',
      imageBase64: photo.toString('base64'),
    };
    expect((await app.inject({ method: 'POST', url, payload })).statusCode).toBe(401);
    const saved = await app.inject({ method: 'POST', url, payload, headers: bearer(one) });
    expect(saved.statusCode).toBe(201);
    expect(saved.json().informationSource).toBe('community');
    expect(saved.json().assessment.status).toBe('CONTAINS_GLUTEN');
    const otherUser = await app.inject('/api/v1/products/barcode/00000000');
    expect(otherUser.json().ingredients).toBe('Harina de trigo');
    const image = await app.inject(url);
    expect(image.statusCode).toBe(200);
    expect(image.headers['content-type']).toContain('image/jpeg');
    expect(image.rawPayload.length).toBeGreaterThan(100);
  });
  it('records separate user reports without changing the ingredient analysis', async () => {
    const url = '/api/v1/products/off-00000000/gluten-report';
    const payload = { status: 'GLUTEN_FREE', note: 'La etiqueta actual dice sin gluten.' };
    expect((await app.inject({ method: 'POST', url, payload })).statusCode).toBe(401);
    const saved = await app.inject({ method: 'POST', url, payload, headers: bearer(one) });
    expect(saved.statusCode).toBe(200);
    expect(saved.json().assessment.status).toBe('CONTAINS_GLUTEN');
    expect(saved.json().glutenReports).toEqual({ glutenFree: 1, containsGluten: 0 });
    const changed = await app.inject({
      method: 'POST',
      url,
      payload: { status: 'CONTAINS_GLUTEN', note: 'Ahora se lee harina de trigo.' },
      headers: bearer(one),
    });
    expect(changed.json().glutenReports).toEqual({ glutenFree: 0, containsGluten: 1 });
    expect((await app.inject('/api/v1/products/off-00000000')).json().glutenReports).toEqual({
      glutenFree: 0,
      containsGluten: 1,
    });
  });
  it('publishes community restaurants and tracks conflicting user reports', async () => {
    const payload = {
      name: `Restaurante de prueba ${suffix}`,
      address: 'Calle de Prueba 12, Madrid',
      latitude: 40.4168,
      longitude: -3.7038,
      support: 'GLUTEN_FREE_OPTIONS',
      note: 'El personal confirmó opciones sin gluten.',
    };
    expect(
      (await app.inject({ method: 'POST', url: '/api/v1/restaurants', payload })).statusCode,
    ).toBe(401);
    const created = await app.inject({
      method: 'POST',
      url: '/api/v1/restaurants',
      payload,
      headers: bearer(one),
    });
    expect(created.statusCode).toBe(201);
    const id = created.json().id;
    expect(id).toMatch(/^se_[a-f0-9]{32}$/);
    expect(created.json().glutenSupport).toBe('GLUTEN_FREE_OPTIONS');
    const nearby = await app.inject(
      '/api/v1/restaurants/search?q=restaurantes&latitude=40.4168&longitude=-3.7038',
    );
    expect(nearby.json().items.some((item: { id: string }) => item.id === id)).toBe(true);
    const named = await app.inject(
      '/api/v1/restaurants/search?q=Integration&latitude=40.4168&longitude=-3.7038',
    );
    expect(named.json().items.map((item: { id: string }) => item.id)).toEqual(['restaurant-1']);
    expect(
      (
        await app.inject(
          '/api/v1/restaurants/search?q=NothingMatchesThis&latitude=40.4168&longitude=-3.7038',
        )
      ).json().items,
    ).toEqual([]);
    const second = await app.inject({
      method: 'POST',
      url: `/api/v1/restaurants/${id}/gluten-report`,
      payload: {
        support: 'NO_GLUTEN_FREE_OPTIONS',
        note: 'El personal dijo que ya no ofrecen esas opciones.',
      },
      headers: bearer(two),
    });
    expect(second.statusCode).toBe(200);
    expect(second.json().glutenSupport).toBe('UNKNOWN');
    expect(second.json().glutenReports).toEqual({ dedicated: 0, options: 1, noOptions: 1 });
  });
  it('creates groups, classifies saved products and isolates membership', async () => {
    const created = await app.inject({
      method: 'POST',
      url: '/api/v1/collections',
      headers: bearer(one),
      payload: { name: 'Desayunos' },
    });
    expect(created.statusCode).toBe(201);
    const id = created.json().id;
    expect(
      (
        await app.inject({
          method: 'POST',
          url: `/api/v1/collections/${id}/products/off-00000000`,
          headers: bearer(one),
        })
      ).statusCode,
    ).toBe(204);
    expect(
      (await app.inject({ url: `/api/v1/collections/${id}/products`, headers: bearer(one) })).json()
        .items,
    ).toHaveLength(1);
    expect(
      (await app.inject({ url: `/api/v1/collections/${id}/products`, headers: bearer(two) })).json()
        .items,
    ).toHaveLength(0);
    expect(
      (
        await app.inject({
          url: '/api/v1/collections/for-product/off-00000000',
          headers: bearer(one),
        })
      ).json().ids,
    ).toContain(id);
    expect(
      (
        await app.inject({
          method: 'DELETE',
          url: `/api/v1/collections/${id}`,
          headers: bearer(one),
        })
      ).statusCode,
    ).toBe(204);
  });
  it('cannot retain an active family during concurrent rotation and logout', async () => {
    const login = await app.inject({
      method: 'POST',
      url: '/api/v1/auth/login',
      payload: { email: one.user.email, password: 'a-strong-password-123' },
    });
    const s: Session = login.json();
    const [rotated, loggedOut] = await Promise.all([
      app.inject({
        method: 'POST',
        url: '/api/v1/auth/refresh',
        payload: { refreshToken: s.refreshToken },
      }),
      app.inject({
        method: 'POST',
        url: '/api/v1/auth/logout',
        payload: { refreshToken: s.refreshToken },
      }),
    ]);
    expect(loggedOut.statusCode).toBe(204);
    expect([200, 401]).toContain(rotated.statusCode);
    if (rotated.statusCode === 200)
      expect(
        (await app.inject({ url: '/api/v1/auth/me', headers: bearer(rotated.json()) })).statusCode,
      ).toBe(401);
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
  it('averages one rating per user and branch, isolates editing, validates stars and removes ratings', async () => {
    const path = '/api/v1/restaurants/chain-rating/rating';
    const put = (stars: number, session = one) =>
      app.inject({ method: 'PUT', url: path, payload: { stars }, headers: bearer(session) });
    expect((await app.inject({ method: 'PUT', url: path, payload: { stars: 5 } })).statusCode).toBe(
      401,
    );
    for (const stars of [0, 6, 2.5]) expect((await put(stars)).statusCode).toBe(400);
    expect((await put(5)).json().rating).toEqual({ average: 5, count: 1 });
    expect((await put(1, two)).json().rating).toEqual({ average: 3, count: 2 });
    expect((await put(3)).json().rating).toEqual({ average: 2, count: 2 });
    expect((await app.inject({ url: path, headers: bearer(two) })).json()).toEqual({ stars: 1 });
    expect((await app.inject(path)).json()).toEqual({ stars: null });
    expect(
      (await app.inject('/api/v1/restaurants/chain-other-branch')).json().rating,
    ).toBeUndefined();
    expect((await app.inject('/api/v1/restaurants/chain-rating')).json().glutenSupport).toBe(
      'UNKNOWN',
    );
    expect(
      (await app.inject({ method: 'DELETE', url: path, headers: bearer(one) })).json().rating,
    ).toEqual({ average: 1, count: 1 });
    expect(
      (await app.inject({ method: 'DELETE', url: path, headers: bearer(two) })).json().rating,
    ).toBeUndefined();
  });
  it('shows reasons, requires another user, rejects stale confirmations and propagates chain reports', async () => {
    const reportUrl = '/api/v1/restaurants/chain-one/gluten-report';
    const post = (url: string, payload: object, session = one) =>
      app.inject({ method: 'POST', url, payload, headers: bearer(session) });
    expect(
      (
        await post(reportUrl, {
          support: 'GLUTEN_FREE_OPTIONS',
          note: 'El personal confirmó una carta sin gluten.',
        })
      ).statusCode,
    ).toBe(200);
    const sibling = await app.inject('/api/v1/restaurants/chain-two');
    expect(sibling.json().glutenSupport).toBe('GLUTEN_FREE_OPTIONS');
    expect((await app.inject('/api/v1/restaurants/unrelated')).json().glutenSupport).toBe(
      'UNKNOWN',
    );
    const reviewsUrl = '/api/v1/restaurants/chain-two/reviews';
    const read = await app.inject(reviewsUrl);
    expect(read.statusCode).toBe(200);
    const review = read.json().items[0];
    expect(review.note).toContain('El personal');
    expect(review.inherited).toBe(true);
    expect(review.verifiedAt).toBeNull();
    expect(read.body).not.toContain(one.user.id);
    const confirmUrl = `${reviewsUrl}/${review.id}/confirm`;
    expect((await post(confirmUrl, { revision: review.revision })).statusCode).toBe(403);
    expect(
      (
        await app.inject({
          method: 'POST',
          url: confirmUrl,
          payload: { revision: review.revision },
        })
      ).statusCode,
    ).toBe(401);
    expect((await post(confirmUrl, { revision: review.revision }, two)).statusCode).toBe(200);
    expect((await post(confirmUrl, { revision: review.revision }, two)).statusCode).toBe(200);
    const confirmed = (await app.inject({ url: reviewsUrl, headers: bearer(two) })).json().items[0];
    expect(confirmed.confirmations).toBe(1);
    expect(confirmed.confirmedByMe).toBe(true);
    expect(confirmed.verifiedAt).toBeTruthy();
    expect(
      (
        await post(
          '/api/v1/restaurants/unrelated/reviews/' + review.id + '/confirm',
          { revision: review.revision },
          two,
        )
      ).statusCode,
    ).toBe(404);
    await post(reportUrl, {
      support: 'DEDICATED_GLUTEN_FREE',
      note: 'El personal confirmó que toda la carta es sin gluten.',
    });
    expect((await post(confirmUrl, { revision: review.revision }, two)).statusCode).toBe(409);
    const changed = (await app.inject(reviewsUrl)).json().items[0];
    expect(changed.verifiedAt).toBeNull();
    expect(changed.revision).not.toBe(review.revision);
    expect((await app.inject('/api/v1/restaurants/chain-two')).json().glutenSupport).toBe(
      'DEDICATED_GLUTEN_FREE',
    );
    await post(
      '/api/v1/restaurants/chain-two/gluten-report',
      {
        support: 'NO_GLUTEN_FREE_OPTIONS',
        note: 'En este local nos indicaron que no hay opciones.',
      },
      two,
    );
    expect((await app.inject('/api/v1/restaurants/chain-two')).json().glutenSupport).toBe(
      'UNKNOWN',
    );
    expect((await app.inject('/api/v1/restaurants/chain-three')).json().glutenSupport).toBe(
      'DEDICATED_GLUTEN_FREE',
    );
    await post('/api/v1/restaurants/chain-four/gluten-report', {
      support: 'GLUTEN_FREE_OPTIONS',
      note: 'He comprobado de nuevo la carta actual de la cadena.',
    });
    expect((await app.inject('/api/v1/restaurants/chain-three')).json().glutenReports).toEqual({
      dedicated: 0,
      options: 1,
      noOptions: 0,
    });
  });
  it('also confirms product reasons and invalidates them when edited', async () => {
    await app.inject('/api/v1/products/barcode/8410000000001');
    const base = '/api/v1/products/off-8410000000001';
    const report = (note: string) =>
      app.inject({
        method: 'POST',
        url: `${base}/gluten-report`,
        headers: bearer(one),
        payload: { status: 'GLUTEN_FREE', note },
      });
    expect((await report('La etiqueta actual indica sin gluten.')).statusCode).toBe(200);
    const review = (await app.inject(`${base}/reviews`))
      .json()
      .items.find((r: { note: string }) => r.note.startsWith('La etiqueta actual'));
    expect(review).toBeTruthy();
    expect(
      (
        await app.inject({
          method: 'POST',
          url: `${base}/reviews/${review.id}/confirm`,
          headers: bearer(two),
          payload: { revision: review.revision },
        })
      ).statusCode,
    ).toBe(200);
    expect(
      (await app.inject(`${base}/reviews`))
        .json()
        .items.find((r: { id: string }) => r.id === review.id).verifiedAt,
    ).toBeTruthy();
    await report('La etiqueta nueva también indica sin gluten.');
    expect(
      (await app.inject(`${base}/reviews`))
        .json()
        .items.find((r: { id: string }) => r.id === review.id).verifiedAt,
    ).toBeNull();
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
  it('rate limits repeated login attempts', async () => {
    const statuses: number[] = [];
    for (let n = 0; n < 11; n++) {
      const response = await app.inject({
        method: 'POST',
        url: '/api/v1/auth/login',
        remoteAddress: '192.0.2.50',
        payload: { email: 'absent@example.org', password: 'not-the-password' },
      });
      statuses.push(response.statusCode);
    }
    expect(statuses.slice(0, 10)).toEqual(Array(10).fill(401));
    expect(statuses[10]).toBe(429);
  });
  it('deletes accounts and associated shares', async () => {
    const login = await app.inject({
      method: 'POST',
      url: '/api/v1/auth/login',
      payload: { email: one.user.email, password: 'a-strong-password-123' },
    });
    const s: Session = login.json();
    expect(
      (
        await app.inject({
          method: 'DELETE',
          url: '/api/v1/auth/me',
          headers: bearer(s),
          payload: { currentPassword: 'a-strong-password-123', confirmation: 'ELIMINAR' },
        })
      ).statusCode,
    ).toBe(204);
    expect((await db.query('SELECT 1 FROM shares WHERE owner_id=$1', [one.user.id])).rowCount).toBe(
      0,
    );
  });
});
