import { afterAll, beforeAll, expect, it } from 'vitest';
import { randomUUID } from 'node:crypto';
import { resolve } from 'node:path';
import type { FastifyInstance } from 'fastify';
import type { Session } from '@safeeat/contracts';
import { createApp } from '../src/app';
import { createPool } from '../src/db/pool';
import { migrate } from '../src/db/migrate';
import { loadEnv } from '../src/config/env';
const db = createPool(process.env.TEST_DATABASE_URL ?? '');
let app: FastifyInstance, admin: Session, author: Session;
const headers = (session: Session) => ({ authorization: `Bearer ${session.accessToken}` });
const password = 'long-admin-test-password';
beforeAll(async () => {
  await migrate(db, resolve('apps/api/src/db/migrations'));
  app = await createApp(
    loadEnv({
      NODE_ENV: 'test',
      DATABASE_URL: process.env.TEST_DATABASE_URL,
      JWT_ACCESS_SECRET: 'admin-access-secret'.repeat(4),
      JWT_REFRESH_SECRET: 'admin-refresh-secret'.repeat(4),
    }),
    {
      db,
      logging: false,
      products: {
        getByBarcode: async (barcode) => ({
          barcode,
          name: 'Admin test food',
          ingredients: 'arroz',
          gluten: { ingredients: 'arroz', source: 'Test', origin: 'provider' },
          nutrition: {},
          source: 'Test',
          sourceUrl: 'https://example.org/food',
        }),
        search: async () => [],
      },
      restaurants: {
        search: async () => [],
        getById: async (id) => ({
          id,
          name: 'Cadena de prueba',
          address: 'Dirección de prueba',
          mapsUrl: 'https://example.org/map',
          chain: { key: 'Q123456789', name: 'Cadena de prueba' },
          glutenSupport: 'UNKNOWN',
          evidence: [],
          source: 'Test',
          attributions: [],
          fetchedAt: new Date().toISOString(),
        }),
      },
    },
  );
  for (const name of ['admin', 'author']) {
    const r = await app.inject({
      method: 'POST',
      url: '/api/v1/auth/register',
      remoteAddress: name === 'admin' ? '192.0.2.80' : '192.0.2.81',
      payload: {
        email: `${name}-${randomUUID()}@example.org`,
        password,
        firstName: name,
        dateOfBirth: '1990-06-15',
      },
    });
    expect(r.statusCode).toBe(201);
    if (name === 'admin') admin = r.json();
    else author = r.json();
  }
  await db.query("INSERT INTO user_roles(user_id,role_code) VALUES($1,'ADMIN')", [admin.user.id]);
});
afterAll(async () => {
  await db.query('DELETE FROM users WHERE id=ANY($1::uuid[])', [
    [admin?.user.id, author?.user.id].filter(Boolean),
  ]);
  await app.close();
  await db.end();
});
it('protects private administration and manages only authorized-celiac memberships with an audit trail', async () => {
  expect((await app.inject('/api/v1/admin/users')).statusCode).toBe(401);
  expect(
    (await app.inject({ url: '/api/v1/admin/users', headers: headers(author) })).statusCode,
  ).toBe(403);
  const list = await app.inject({
    url: `/api/v1/admin/users?q=${encodeURIComponent(author.user.email)}`,
    headers: headers(admin),
  });
  expect(list.statusCode).toBe(200);
  expect(list.json().items).toHaveLength(1);
  expect(list.body).not.toContain('password_hash');
  expect(
    (await app.inject({ url: '/api/v1/auth/me', headers: headers(admin) })).json().roles,
  ).toEqual(['ADMIN']);
  const path = `/api/v1/admin/users/${author.user.id}/authorized`;
  expect(
    (await app.inject({ method: 'PUT', url: path, headers: headers(author) })).statusCode,
  ).toBe(403);
  expect((await app.inject({ method: 'PUT', url: path, headers: headers(admin) })).json()).toEqual({
    authorized: true,
  });
  await app.inject({ method: 'PUT', url: path, headers: headers(admin) });
  expect(
    (await app.inject({ url: '/api/v1/auth/me', headers: headers(author) })).json().roles,
  ).toEqual(['authorized_celiac']);
  expect(
    (
      await db.query(
        "SELECT action FROM admin_audit_log WHERE target_id=$1 AND action='authorized_granted'",
        [author.user.id],
      )
    ).rowCount,
  ).toBe(1);
  expect(
    (await app.inject({ method: 'DELETE', url: path, headers: headers(admin) })).json(),
  ).toEqual({ authorized: false });
});
it('accepts missing and short reasons only for authorized reviewers, in both kinds and new restaurants', async () => {
  await app.inject('/api/v1/products/barcode/3099999999999');
  const cases = [
    ['restaurants', 'chain-admin-a', { support: 'GLUTEN_FREE_OPTIONS', scope: 'chain' }],
    ['products', 'off-3099999999999', { status: 'GLUTEN_FREE' }],
  ] as const;
  for (const [kind, id, payload] of cases) {
    expect(
      (
        await app.inject({
          method: 'POST',
          url: `/api/v1/${kind}/${id}/gluten-report`,
          headers: headers(author),
          payload,
        })
      ).statusCode,
    ).toBe(400);
  }
  await app.inject({
    method: 'PUT',
    url: `/api/v1/admin/users/${author.user.id}/authorized`,
    headers: headers(admin),
  });
  for (const [kind, id, payload] of cases) {
    const written = await app.inject({
      method: 'POST',
      url: `/api/v1/${kind}/${id}/gluten-report`,
      headers: headers(author),
      payload,
    });
    expect(written.statusCode).toBe(200);
    const review = (await app.inject(`/api/v1/${kind}/${id}/reviews`)).json().items[0];
    expect(review.note).toBe('');
    expect(review.verificationMethod).toBe('authorized');
  }
  expect(
    (
      await app.inject({
        method: 'POST',
        url: '/api/v1/restaurants',
        headers: headers(author),
        payload: {
          name: 'Nuevo local de prueba',
          address: 'Dirección de prueba',
          latitude: 40.4,
          longitude: -3.7,
          support: 'GLUTEN_FREE_OPTIONS',
        },
      })
    ).statusCode,
  ).toBe(201);
  await app.inject({
    method: 'DELETE',
    url: `/api/v1/admin/users/${author.user.id}/authorized`,
    headers: headers(admin),
  });
  expect(
    (
      await app.inject({
        method: 'POST',
        url: '/api/v1/products/off-3099999999999/gluten-report',
        headers: headers(author),
        payload: { status: 'GLUTEN_FREE', note: 'breve' },
      })
    ).statusCode,
  ).toBe(400);
});
it('revokes and restores reviews safely, hides chain inheritance, blocks republication, and records decisions', async () => {
  const productReviews = await app.inject('/api/v1/products/off-3099999999999/reviews');
  const productReview = productReviews.json().items[0];
  expect(productReview).toBeTruthy();
  expect(
    (
      await app.inject({
        method: 'POST',
        url: `/api/v1/products/off-3099999999999/reviews/${productReview.id}/confirm`,
        headers: headers(admin),
        payload: { revision: productReview.revision },
      })
    ).statusCode,
  ).toBe(403);
  const verified = await app.inject({
    method: 'POST',
    url: `/api/v1/admin/reviews/products/${productReview.id}/verify`,
    headers: headers(admin),
    payload: { revision: productReview.revision },
  });
  expect(verified.statusCode).toBe(200);
  expect(
    (await app.inject('/api/v1/products/off-3099999999999/reviews')).json().items[0],
  ).toMatchObject({
    verificationMethod: 'manual',
    confirmations: 1,
  });
  expect(
    (
      await db.query(
        "SELECT action FROM admin_audit_log WHERE target_id=$1 AND action='product_review_verified'",
        [productReview.id],
      )
    ).rowCount,
  ).toBe(1);
  for (const [kind, id] of [
    ['restaurants', 'chain-admin-a'],
    ['products', 'off-3099999999999'],
  ] as const) {
    let review = (await app.inject(`/api/v1/${kind}/${id}/reviews`)).json().items[0];
    const path = `/api/v1/admin/reviews/${kind}/${review.id}/moderate`;
    const payload = {
      revision: review.revision,
      action: 'revoke',
      reason: 'Aportación incorrecta comprobada.',
    };
    expect(
      (await app.inject({ method: 'POST', url: path, headers: headers(author), payload }))
        .statusCode,
    ).toBe(403);
    expect(
      (
        await app.inject({
          method: 'POST',
          url: path,
          headers: headers(admin),
          payload: { ...payload, revision: randomUUID() },
        })
      ).statusCode,
    ).toBe(409);
    const revoked = await app.inject({
      method: 'POST',
      url: path,
      headers: headers(admin),
      payload,
    });
    expect(revoked.statusCode).toBe(200);
    expect((await app.inject(`/api/v1/${kind}/${id}/reviews`)).json().items).toEqual([]);
    expect(
      (
        await app.inject({
          method: 'POST',
          url: `/api/v1/${kind}/${id}/reviews/${review.id}/confirm`,
          headers: headers(admin),
          payload: { revision: revoked.json().revision },
        })
      ).statusCode,
    ).toBe(kind === 'products' ? 403 : 404);
    const body =
      kind === 'restaurants' ? { support: 'GLUTEN_FREE_OPTIONS' } : { status: 'GLUTEN_FREE' };
    expect(
      (
        await app.inject({
          method: 'POST',
          url: `/api/v1/${kind}/${id}/gluten-report`,
          headers: headers(author),
          remoteAddress: '192.0.2.82',
          payload: { ...body, note: 'No puede volver a publicarse sin moderación.' },
        })
      ).statusCode,
    ).toBe(409);
    if (kind === 'restaurants')
      expect((await app.inject('/api/v1/restaurants/chain-admin-peer')).json().glutenSupport).toBe(
        'UNKNOWN',
      );
    else
      expect((await app.inject(`/api/v1/${kind}/${id}`)).json().glutenReports.glutenFree).toBe(0);
    const hidden = (
      await app.inject({
        url: `/api/v1/admin/reviews?kind=${kind}&state=revoked`,
        headers: headers(admin),
      })
    )
      .json()
      .items.find((r: { id: string }) => r.id === review.id);
    expect(hidden.revocationReason).toBe(payload.reason);
    expect(
      (
        await app.inject({
          method: 'POST',
          url: path,
          headers: headers(admin),
          payload: {
            revision: hidden.revision,
            action: 'restore',
            reason: 'Nueva comprobación de la aportación.',
          },
        })
      ).statusCode,
    ).toBe(200);
    review = (await app.inject(`/api/v1/${kind}/${id}/reviews`)).json().items[0];
    expect(review.revision).not.toBe(payload.revision);
    expect(
      (
        await db.query(
          "SELECT 1 FROM admin_audit_log WHERE target_id=$1 AND action='review_revoked'",
          [review.id],
        )
      ).rowCount,
    ).toBe(1);
  }
});
it('checks current admin permissions instead of trusting the existing session', async () => {
  await db.query("DELETE FROM user_roles WHERE user_id=$1 AND role_code='ADMIN'", [admin.user.id]);
  expect(
    (await app.inject({ url: '/api/v1/admin/users', headers: headers(admin) })).statusCode,
  ).toBe(403);
  expect(
    (
      await app.inject({
        method: 'PUT',
        url: `/api/v1/admin/users/${author.user.id}/authorized`,
        headers: headers(admin),
      })
    ).statusCode,
  ).toBe(403);
});
