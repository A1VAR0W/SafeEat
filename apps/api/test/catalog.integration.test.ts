import { afterAll, beforeAll, describe, expect, it } from 'vitest';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { resolve } from 'node:path';
import { createPool } from '../src/db/pool';
import { migrate } from '../src/db/migrate';
import { restaurantCatalog } from '../src/modules/restaurants/catalog';

const url = process.env.TEST_DATABASE_URL;
if (!url) throw new Error('TEST_DATABASE_URL is required');
const db = createPool(url);
const catalog = restaurantCatalog(db);
const execFileAsync = promisify(execFile);

beforeAll(async () => {
  await migrate(db, resolve('apps/api/src/db/migrations'));
  await execFileAsync(
    process.execPath,
    [
      'scripts/import-restaurant-catalog.mjs',
      'testregion',
      'apps/api/test/fixtures/restaurants.geojsonseq',
    ],
    { cwd: resolve('.'), env: { ...process.env, DATABASE_URL: url } },
  );
});
afterAll(async () => {
  await db.query("DELETE FROM restaurant_catalog WHERE region='testregion'");
  await db.query("DELETE FROM restaurant_catalog_imports WHERE region='testregion'");
  await db.end();
});

describe('local restaurant catalog', () => {
  it('searches only matching branches using the brand identity and ignores unrelated nearby venues', async () => {
    const items = await catalog.search({
      query: 'Cadena Prueba',
      latitude: 40.42,
      longitude: -3.71,
    });
    expect(items.map((item) => item.id).sort()).toEqual([
      'osm_n_987654324',
      'osm_n_987654325',
      'osm_n_987654326',
    ]);
    expect(
      (await catalog.search({ query: 'Casa Sin Gluten', latitude: 40.42, longitude: -3.71 })).map(
        (item) => item.id,
      ),
    ).toEqual(['osm_n_987654321']);
  });
  it('prioritizes matching branches near the map and exposes their identity for direct selection', async () => {
    const items = await catalog.suggest('Cadena', { latitude: 40.42, longitude: -3.71 });
    expect(items[0]?.id).toBe('osm_n_987654325');
    expect(items[0]?.longitude).toBe(-3.71);
  });
  it('imports chain identity and includes sibling branches in dietary searches', async () => {
    expect((await catalog.get('osm_n_987654326'))?.chain).toEqual({
      key: 'Q999999998',
      name: 'Cadena Prueba',
    });
    expect((await catalog.get('osm_n_987654327'))?.chain).toBeUndefined();
    expect((await catalog.get('osm_n_987654324'))?.chain).toEqual({
      key: 'Q999999998',
      name: 'Cadena Prueba',
    });
    const user = (
      await db.query(
        "INSERT INTO users(id,email,password_hash) VALUES(gen_random_uuid(),'catalog-chain-test@example.org','test-only') RETURNING id",
      )
    ).rows[0].id;
    try {
      await db.query(
        "INSERT INTO restaurant_gluten_reports(restaurant_id,user_id,support,note,chain_key) VALUES('osm_n_987654324',$1,'GLUTEN_FREE_OPTIONS','Carta con opciones sin gluten','Q999999998')",
        [user],
      );
      const params = { query: 'restaurantes', latitude: 40.4168, longitude: -3.7038 };
      expect(
        (await catalog.search({ ...params, glutenMode: 'options' })).map((r) => r.id),
      ).toContain('osm_n_987654325');
      expect(
        (await catalog.search({ ...params, glutenMode: 'options' })).map((r) => r.id),
      ).toContain('osm_n_987654326');
      expect(
        (await catalog.search({ ...params, glutenMode: 'dedicated' })).map((r) => r.id),
      ).not.toContain('osm_n_987654325');
    } finally {
      await db.query('DELETE FROM users WHERE id=$1', [user]);
    }
  });
  it('serves the map without an external provider and keeps dietary filters distinct', async () => {
    const nearby = { query: 'restaurantes', latitude: 40.4168, longitude: -3.7038 };
    const all = await catalog.search(nearby);
    expect(all.map((item) => item.id)).toEqual(
      expect.arrayContaining(['osm_n_987654321', 'osm_w_987654322', 'osm_n_987654323']),
    );
    const dedicated = await catalog.search({ ...nearby, glutenMode: 'dedicated' });
    expect(dedicated.map((item) => item.id)).toContain('osm_n_987654321');
    expect(dedicated.map((item) => item.id)).not.toContain('osm_w_987654322');
    const options = await catalog.search({ ...nearby, glutenMode: 'options' });
    expect(options.map((item) => item.id)).toEqual(
      expect.arrayContaining(['osm_n_987654321', 'osm_w_987654322']),
    );
    expect(options.map((item) => item.id)).not.toContain('osm_n_987654323');
  });
});
