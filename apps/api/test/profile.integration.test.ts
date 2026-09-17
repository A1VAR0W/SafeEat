import { beforeAll, afterAll, expect, it } from 'vitest';
import { randomUUID } from 'node:crypto';
import { readFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import argon2 from 'argon2';
import type { FastifyInstance, InjectOptions } from 'fastify';
import type { Session } from '@safeeat/contracts';
import { createPool } from '../src/db/pool';
import { migrate } from '../src/db/migrate';
import { createApp } from '../src/app';
import { loadEnv } from '../src/config/env';
const url = process.env.TEST_DATABASE_URL;
if (!url) throw new Error('Use an isolated TEST_DATABASE_URL');
const db = createPool(url),
  suffix = randomUUID(),
  password = 'strong-password-for-v2-tests';
let app: FastifyInstance,
  sequence = 0;
const userIds: string[] = [];
const bearer = (s: Session) => ({ authorization: `Bearer ${s.accessToken}` });
const inject = (options: InjectOptions) =>
  app.inject({ ...options, remoteAddress: `192.0.2.${(++sequence % 254) + 1}` });
async function register(optIn = false) {
  const response = await inject({
    method: 'POST',
    url: '/api/v1/auth/register',
    payload: {
      email: `v2-${sequence}-${suffix}@example.org`,
      password,
      firstName: 'Ana',
      dateOfBirth: '1994-03-21',
      productUpdatesConsent: optIn,
    },
  });
  expect(response.statusCode).toBe(201);
  const s: Session = response.json();
  userIds.push(s.user.id);
  return s;
}
beforeAll(async () => {
  await migrate(db, resolve('apps/api/src/db/migrations'));
  app = await createApp(
    loadEnv({
      NODE_ENV: 'test',
      DATABASE_URL: url,
      JWT_ACCESS_SECRET: 'v2-test-access-secret-'.repeat(4),
      JWT_REFRESH_SECRET: 'v2-test-refresh-secret-'.repeat(4),
    }),
    { db, logging: false },
  );
  await app.ready();
});
afterAll(async () => {
  if (app) await app.close();
  await db.query('DELETE FROM shares WHERE resource_id=$1', [`v2-public-${suffix}`]);
  await db.query('DELETE FROM products WHERE id=$1', [`v2-public-${suffix}`]);
  await db.query('DELETE FROM users WHERE id=ANY($1::uuid[])', [userIds]);
  await db.end();
});
it('migrates a populated V1 schema without destroying accounts or inventing personal data', async () => {
  const client = await db.connect();
  try {
    await client.query('BEGIN');
    const schema = `v2_${suffix.replaceAll('-', '')}`;
    await client.query(`CREATE SCHEMA ${schema}`);
    await client.query(`SET LOCAL search_path TO ${schema}`);
    await client.query(await readFile('apps/api/src/db/migrations/001_initial.sql', 'utf8'));
    const id = randomUUID();
    await client.query('INSERT INTO users(id,email,password_hash) VALUES($1,$2,$3)', [
      id,
      'legacy@example.org',
      'existing-hash',
    ]);
    await client.query(await readFile('apps/api/src/db/migrations/002_user_profile.sql', 'utf8'));
    const row = (await client.query('SELECT * FROM users WHERE id=$1', [id])).rows[0];
    expect(row).toMatchObject({
      email: 'legacy@example.org',
      password_hash: 'existing-hash',
      first_name: null,
      date_of_birth: null,
      product_updates_consent: false,
    });
  } finally {
    await client.query('ROLLBACK');
    client.release();
  }
});
it('registers optional surname and separate timestamped opt-in without leaking password hashes', async () => {
  const ordinary = await register(),
    opted = await register(true);
  expect(ordinary.user).toMatchObject({
    firstName: 'Ana',
    lastName: null,
    dateOfBirth: '1994-03-21',
    productUpdatesConsent: false,
    productUpdatesConsentAt: null,
  });
  expect(opted.user.productUpdatesConsentAt).toBeTruthy();
  expect(opted.user.productUpdatesConsentVersion).toBe('2026-09-v1');
  expect(JSON.stringify(opted)).not.toContain('password_hash');
  for (const date of ['2030-01-01', '2001-02-29'])
    expect(
      (
        await inject({
          method: 'POST',
          url: '/api/v1/auth/register',
          payload: {
            email: `bad-${sequence}@example.org`,
            password,
            firstName: 'Ana',
            dateOfBirth: date,
          },
        })
      ).statusCode,
    ).toBe(400);
});
it('allows legacy login and one-time completion, then locks personal fields', async () => {
  const id = randomUUID(),
    email = `legacy-${suffix}@example.org`;
  userIds.push(id);
  await db.query('INSERT INTO users(id,email,password_hash) VALUES($1,$2,$3)', [
    id,
    email,
    await argon2.hash(password),
  ]);
  const login = await inject({
    method: 'POST',
    url: '/api/v1/auth/login',
    payload: { email, password },
  });
  expect(login.statusCode).toBe(200);
  const s: Session = login.json();
  expect(s.user.firstName).toBeNull();
  const payload = { firstName: 'Legacy', dateOfBirth: '1980-01-02' };
  expect(
    (
      await inject({
        method: 'POST',
        url: '/api/v1/auth/profile/complete',
        headers: bearer(s),
        payload,
      })
    ).statusCode,
  ).toBe(200);
  expect(
    (
      await inject({
        method: 'POST',
        url: '/api/v1/auth/profile/complete',
        headers: bearer(s),
        payload: { ...payload, firstName: 'Replaced' },
      })
    ).statusCode,
  ).toBe(409);
  expect((await inject({ url: '/api/v1/auth/me', headers: bearer(s) })).json().firstName).toBe(
    'Legacy',
  );
});
it('requires reauthentication and uniqueness for email changes and revokes older sessions', async () => {
  const a = await register(),
    b = await register();
  for (const [payload, status] of [
    [{ email: b.user.email, currentPassword: password }, 409],
    [{ email: 'valid@example.org', currentPassword: 'wrong' }, 403],
    [{ email: 'valid@example.org', currentPassword: password, firstName: 'Override' }, 400],
  ] as const) {
    expect(
      (await inject({ method: 'POST', url: '/api/v1/auth/email', headers: bearer(a), payload }))
        .statusCode,
    ).toBe(status);
  }
  expect((await inject({ url: '/api/v1/auth/me', headers: bearer(a) })).statusCode).toBe(200);
  const email = `changed-${suffix}@example.org`;
  const changed = await inject({
    method: 'POST',
    url: '/api/v1/auth/email',
    headers: bearer(a),
    payload: { email, currentPassword: password },
  });
  expect(changed.statusCode).toBe(200);
  expect(changed.json().user.email).toBe(email);
  expect((await inject({ url: '/api/v1/auth/me', headers: bearer(a) })).statusCode).toBe(401);
  expect(
    (
      await inject({
        method: 'POST',
        url: '/api/v1/auth/refresh',
        payload: { refreshToken: a.refreshToken },
      })
    ).statusCode,
  ).toBe(401);
  expect(
    (await inject({ url: '/api/v1/auth/me', headers: bearer(changed.json()) })).statusCode,
  ).toBe(200);
});
it('changes passwords without allowing a racing refresh or old-password login to escape revocation', async () => {
  const s = await register(),
    newPassword = 'replacement-password-for-v2';
  const other: Session = (
    await inject({
      method: 'POST',
      url: '/api/v1/auth/login',
      payload: { email: s.user.email, password },
    })
  ).json();
  const [change, refresh, login] = await Promise.all([
    inject({
      method: 'POST',
      url: '/api/v1/auth/password',
      headers: bearer(s),
      payload: { currentPassword: password, newPassword },
    }),
    inject({
      method: 'POST',
      url: '/api/v1/auth/refresh',
      payload: { refreshToken: other.refreshToken },
    }),
    inject({
      method: 'POST',
      url: '/api/v1/auth/login',
      payload: { email: s.user.email, password },
    }),
  ]);
  expect(change.statusCode).toBe(200);
  for (const result of [refresh, login]) {
    expect([200, 401]).toContain(result.statusCode);
    if (result.statusCode === 200)
      expect(
        (await inject({ url: '/api/v1/auth/me', headers: bearer(result.json()) })).statusCode,
      ).toBe(401);
  }
  expect(
    (
      await inject({
        method: 'POST',
        url: '/api/v1/auth/login',
        payload: { email: s.user.email, password },
      })
    ).statusCode,
  ).toBe(401);
  expect(
    (
      await inject({
        method: 'POST',
        url: '/api/v1/auth/login',
        payload: { email: s.user.email, password: newPassword },
      })
    ).statusCode,
  ).toBe(200);
});
it('bulk-clears only the current user saved data and revokes personal-label links', async () => {
  const a = await register(),
    b = await register();
  const labels = [];
  for (const s of [a, b]) {
    const label = (
      await inject({
        method: 'POST',
        url: '/api/v1/products/analyze-text',
        headers: bearer(s),
        payload: { name: 'Personal label', text: 'Puede contener trigo', reviewed: true },
      })
    ).json();
    labels.push(label);
    await db.query('INSERT INTO favorite_products(user_id,product_id) VALUES($1,$2)', [
      s.user.id,
      label.id,
    ]);
    await db.query('INSERT INTO favorite_restaurants(user_id,restaurant_id) VALUES($1,$2)', [
      s.user.id,
      'fixture-place',
    ]);
  }
  const link = (
    await inject({
      method: 'POST',
      url: `/api/v1/shares/products/${labels[0].id}`,
      headers: bearer(a),
    })
  ).json();
  await db.query(
    "INSERT INTO products(id,provider,data,expires_at) VALUES($1,'fixture',$2,now()+interval '1 day')",
    [
      `v2-public-${suffix}`,
      JSON.stringify({ ...labels[0], id: `v2-public-${suffix}`, userCreated: false }),
    ],
  );
  expect(
    (await inject({ method: 'DELETE', url: '/api/v1/saved', headers: bearer(a) })).statusCode,
  ).toBe(400);
  expect(
    (
      await inject({
        method: 'DELETE',
        url: '/api/v1/saved',
        headers: bearer(a),
        payload: { confirmation: 'BORRAR' },
      })
    ).statusCode,
  ).toBe(204);
  expect((await inject({ url: `/api/v1/public/shares/${link.token}` })).statusCode).toBe(404);
  expect((await db.query('SELECT id FROM products WHERE owner_id=$1', [a.user.id])).rowCount).toBe(
    0,
  );
  expect((await db.query('SELECT id FROM products WHERE owner_id=$1', [b.user.id])).rowCount).toBe(
    1,
  );
  expect(
    (await db.query('SELECT id FROM products WHERE id=$1', [`v2-public-${suffix}`])).rowCount,
  ).toBe(1);
  for (const table of ['favorite_products', 'favorite_restaurants']) {
    expect((await db.query(`SELECT 1 FROM ${table} WHERE user_id=$1`, [a.user.id])).rowCount).toBe(
      0,
    );
    expect((await db.query(`SELECT 1 FROM ${table} WHERE user_id=$1`, [b.user.id])).rowCount).toBe(
      1,
    );
  }
  expect((await inject({ url: '/api/v1/auth/me', headers: bearer(a) })).statusCode).toBe(200);
});
it('deletes an account only after explicit confirmation and password reauthentication', async () => {
  const s = await register();
  for (const [payload, status] of [
    [{}, 400],
    [{ confirmation: 'ELIMINAR', currentPassword: 'wrong' }, 403],
  ] as const)
    expect(
      (await inject({ method: 'DELETE', url: '/api/v1/auth/me', headers: bearer(s), payload }))
        .statusCode,
    ).toBe(status);
  expect(
    (
      await inject({
        method: 'DELETE',
        url: '/api/v1/auth/me',
        headers: bearer(s),
        payload: { confirmation: 'ELIMINAR', currentPassword: password },
      })
    ).statusCode,
  ).toBe(204);
  expect((await inject({ url: '/api/v1/auth/me', headers: bearer(s) })).statusCode).toBe(401);
  expect(
    (await db.query('SELECT id FROM refresh_sessions WHERE user_id=$1', [s.user.id])).rowCount,
  ).toBe(0);
});
