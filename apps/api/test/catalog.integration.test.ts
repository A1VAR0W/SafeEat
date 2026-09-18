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
