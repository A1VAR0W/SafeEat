// Import an osmium GeoJSON sequence. The replacement for one region is atomic.
import { createReadStream } from 'node:fs';
import { createInterface } from 'node:readline';
import pg from 'pg';

const [region, file] = process.argv.slice(2);
if (!/^[a-z][a-z0-9_-]{1,40}$/.test(region ?? '') || !file || !process.env.DATABASE_URL)
  throw new Error(
    'Usage: DATABASE_URL=... node scripts/import-restaurant-catalog.mjs <region> <geojsonseq-file>',
  );

const amenities = new Set([
  'restaurant',
  'cafe',
  'fast_food',
  'bar',
  'pub',
  'food_court',
  'biergarten',
]);
const shops = new Set(['bakery', 'pastry', 'convenience']);
function point(geometry) {
  if (!geometry?.coordinates) return null;
  if (geometry.type === 'Point') return geometry.coordinates;
  const pairs = [];
  function collect(value) {
    if (!Array.isArray(value)) return;
    if (typeof value[0] === 'number' && typeof value[1] === 'number') pairs.push(value);
    else for (const child of value) collect(child);
  }
  collect(geometry.coordinates);
  if (!pairs.length) return null;
  return [
    (Math.min(...pairs.map((p) => p[0])) + Math.max(...pairs.map((p) => p[0]))) / 2,
    (Math.min(...pairs.map((p) => p[1])) + Math.max(...pairs.map((p) => p[1]))) / 2,
  ];
}
function record(feature) {
  const tags = feature.properties ?? {};
  const kind = { node: 'n', way: 'w', relation: 'r' }[tags['@type']];
  const id = Number(tags['@id']);
  const category = amenities.has(tags.amenity)
    ? tags.amenity
    : shops.has(tags.shop)
      ? tags.shop
      : null;
  const coordinates = point(feature.geometry);
  if (!kind || !Number.isSafeInteger(id) || id <= 0 || !category || !tags.name || !coordinates)
    return null;
  const [longitude, latitude] = coordinates;
  if (
    !Number.isFinite(longitude) ||
    !Number.isFinite(latitude) ||
    Math.abs(latitude) > 90 ||
    Math.abs(longitude) > 180
  )
    return null;
  const website = tags.website || tags['contact:website'];
  return [
    `osm_${kind}_${id}`,
    region,
    String(tags.name).slice(0, 200),
    [tags['addr:street'], tags['addr:housenumber'], tags['addr:city']]
      .filter(Boolean)
      .join(', ')
      .slice(0, 300),
    category,
    longitude,
    latitude,
    /^https?:\/\//i.test(website ?? '') ? website : null,
    tags.phone || tags['contact:phone'] || null,
    tags['diet:gluten_free'] || null,
    /^Q\d+$/.test(tags['brand:wikidata'] ?? '') ? tags['brand:wikidata'] : null,
    tags.brand || null,
  ];
}

const db = new pg.Client({ connectionString: process.env.DATABASE_URL });
await db.connect();
try {
  await db.query('BEGIN');
  await db.query('SELECT pg_advisory_xact_lock(87349103)');
  await db.query(
    'CREATE TEMP TABLE incoming_restaurants (LIKE restaurant_catalog INCLUDING DEFAULTS) ON COMMIT DROP',
  );
  await db.query('ALTER TABLE incoming_restaurants ADD PRIMARY KEY(id)');
  let count = 0;
  let batch = [];
  async function flush() {
    if (!batch.length) return;
    const values = batch.flat();
    const placeholders = batch
      .map(
        (_, i) =>
          `(${Array.from({ length: 5 }, (_, j) => `$${i * 12 + j + 1}`).join(',')},ST_SetSRID(ST_MakePoint($${i * 12 + 6},$${i * 12 + 7}),4326)::geography,$${i * 12 + 8},$${i * 12 + 9},$${i * 12 + 10},$${i * 12 + 11},$${i * 12 + 12})`,
      )
      .join(',');
    await db.query(
      `INSERT INTO incoming_restaurants(id,region,name,address,category,location,website,phone,gluten_tag,chain_key,chain_name) VALUES ${placeholders} ON CONFLICT(id) DO NOTHING`,
      values,
    );
    batch = [];
  }
  for await (const line of createInterface({
    input: createReadStream(file),
    crlfDelay: Infinity,
  })) {
    if (!line.trim()) continue;
    const item = record(JSON.parse(line.charCodeAt(0) === 30 ? line.slice(1) : line));
    if (!item) continue;
    batch.push(item);
    count++;
    if (batch.length === 200) await flush();
  }
  await flush();
  if (!count) throw new Error('No food establishments found; keeping the existing catalog');
  await db.query('DELETE FROM restaurant_catalog WHERE region=$1', [region]);
  await db.query(
    'INSERT INTO restaurant_catalog(id,region,name,address,category,location,website,phone,gluten_tag,chain_key,chain_name) SELECT id,region,name,address,category,location,website,phone,gluten_tag,chain_key,chain_name FROM incoming_restaurants ON CONFLICT(id) DO UPDATE SET region=EXCLUDED.region,name=EXCLUDED.name,address=EXCLUDED.address,category=EXCLUDED.category,location=EXCLUDED.location,website=EXCLUDED.website,phone=EXCLUDED.phone,gluten_tag=EXCLUDED.gluten_tag,chain_key=EXCLUDED.chain_key,chain_name=EXCLUDED.chain_name,imported_at=now()',
  );
  await db.query(
    'INSERT INTO restaurant_catalog_imports(region,item_count) VALUES($1,$2) ON CONFLICT(region) DO UPDATE SET imported_at=now(),item_count=$2',
    [region, count],
  );
  // Schema 2.0 (migration 010) recomputes chains from the raw OSM brand tags, kept in
  // their own columns. Older databases do not have them yet.
  const rawBrand = await db.query(
    "SELECT 1 FROM information_schema.columns WHERE table_name='restaurant_catalog' AND column_name='brand_wikidata'",
  );
  if (rawBrand.rowCount)
    await db.query(
      'UPDATE restaurant_catalog SET brand=chain_name,brand_wikidata=chain_key WHERE region=$1',
      [region],
    );
  // Resolve exact brand aliases before extending positive reports to known siblings.
  await db.query('SELECT refresh_restaurant_chains()');
  // Preserve the original reason while extending existing positive reports to known siblings.
  await db.query(
    "UPDATE restaurant_gluten_reports r SET chain_key=c.chain_key FROM restaurant_catalog c WHERE r.restaurant_id=c.id AND c.region=$1 AND c.chain_key IS NOT NULL AND r.chain_key IS DISTINCT FROM c.chain_key AND r.scope='chain'",
    [region],
  );
  await db.query('COMMIT');
  console.log(`Imported ${count} ${region} restaurants`);
} catch (error) {
  await db.query('ROLLBACK');
  throw error;
} finally {
  await db.end();
}
