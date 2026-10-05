import { RestaurantSchema, type Restaurant } from '@safeeat/contracts';
import type { Database } from '../../db/pool';
import type { RestaurantSearch, RestaurantSuggestion } from '../../providers/types';

type CatalogRow = {
  id: string;
  name: string;
  address: string;
  latitude: number;
  longitude: number;
  website: string | null;
  phone: string | null;
  gluten_tag: string | null;
  chain_key?: string | null;
  chain_name?: string | null;
  imported_at: Date;
};

export function catalogRestaurant(row: CatalogRow): Restaurant {
  const [, kind, osmId] = /^osm_([nwr])_(\d+)$/.exec(row.id)!;
  const osmKind = { n: 'node', w: 'way', r: 'relation' }[kind as 'n' | 'w' | 'r'];
  const glutenSupport =
    row.gluten_tag === 'only'
      ? 'DEDICATED_GLUTEN_FREE'
      : row.gluten_tag === 'yes'
        ? 'GLUTEN_FREE_OPTIONS'
        : row.gluten_tag === 'no'
          ? 'NO_GLUTEN_FREE_OPTIONS'
          : 'UNKNOWN';
  return RestaurantSchema.parse({
    id: row.id,
    name: row.name,
    address: row.address,
    latitude: row.latitude,
    longitude: row.longitude,
    website: row.website || undefined,
    phone: row.phone || undefined,
    mapsUrl: `https://www.openstreetmap.org/${osmKind}/${osmId}`,
    glutenSupport,
    chain: row.chain_key ? { key: row.chain_key, name: row.chain_name || row.name } : undefined,
    evidence: [
      {
        source: 'OpenStreetMap',
        kind: 'uncertainty',
        text:
          glutenSupport === 'UNKNOWN'
            ? row.gluten_tag === 'limited'
              ? 'La ficha indica opciones limitadas. Consulta el menú y la preparación.'
              : 'Aún no hay información confirmada sobre opciones sin gluten.'
            : glutenSupport === 'DEDICATED_GLUTEN_FREE'
              ? 'La ficha indica que el local ofrece prácticamente todo sin gluten. Confirma la preparación antes de acudir.'
              : glutenSupport === 'GLUTEN_FREE_OPTIONS'
                ? 'La ficha indica opciones sin gluten. Confirma ingredientes y contaminación cruzada.'
                : 'La ficha indica que no ofrece opciones sin gluten.',
      },
    ],
    source: 'OpenStreetMap · ODbL',
    attributions: ['© OpenStreetMap contributors'],
    fetchedAt: row.imported_at.toISOString(),
  });
}

const fields =
  'id,name,address,ST_Y(location::geometry) AS latitude,ST_X(location::geometry) AS longitude,website,phone,gluten_tag,chain_key,chain_name,imported_at';

export function restaurantCatalog(db: Database) {
  async function ready() {
    const result = await db.query<{ exists: boolean }>(
      'SELECT EXISTS(SELECT 1 FROM restaurant_catalog_imports) AS exists',
    );
    return result.rows[0]?.exists ?? false;
  }

  async function get(id: string) {
    const result = await db.query<CatalogRow>(
      `SELECT ${fields} FROM restaurant_catalog WHERE id=$1`,
      [id],
    );
    return result.rows[0] ? catalogRestaurant(result.rows[0]) : null;
  }

  async function search(params: RestaurantSearch): Promise<Restaurant[]> {
    const nearby = params.latitude !== undefined && params.longitude !== undefined;
    const broad = /^(restaurantes?|cerca de m[ií])$/i.test(params.query);
    if (broad && !nearby) return [];
    const filter =
      params.glutenMode === 'dedicated'
        ? "AND (gluten_tag='only' OR EXISTS(SELECT 1 FROM restaurant_gluten_reports r WHERE r.revoked_at IS NULL AND (r.restaurant_id=restaurant_catalog.id OR r.chain_key=restaurant_catalog.chain_key OR r.chain_key='name:'||restaurant_brand_name(restaurant_catalog.name)) AND r.support='DEDICATED_GLUTEN_FREE'))"
        : params.glutenFree || params.glutenMode === 'options'
          ? "AND (gluten_tag IN ('only','yes') OR EXISTS(SELECT 1 FROM restaurant_gluten_reports r WHERE r.revoked_at IS NULL AND (r.restaurant_id=restaurant_catalog.id OR r.chain_key=restaurant_catalog.chain_key OR r.chain_key='name:'||restaurant_brand_name(restaurant_catalog.name)) AND r.support IN ('DEDICATED_GLUTEN_FREE','GLUTEN_FREE_OPTIONS')))"
          : '';
    const point = 'ST_SetSRID(ST_MakePoint($2,$3),4326)::geography';
    const location = nearby ? `AND ST_DWithin(location,${point},$4)` : '';
    const name = broad
      ? 'AND $1::text IS NOT NULL'
      : `AND CASE
      WHEN EXISTS(SELECT 1 FROM restaurant_chain_aliases WHERE alias=restaurant_brand_name($1))
      THEN chain_key=(SELECT chain_key FROM restaurant_chain_aliases WHERE alias=restaurant_brand_name($1))
      ELSE position(restaurant_brand_name($1) in restaurant_brand_name(name))>0 END`;
    const order = nearby
      ? `ORDER BY ${broad ? '' : 'similarity(name,$1) DESC,'} location <-> ${point}`
      : 'ORDER BY similarity(name,$1) DESC, name';
    const values = nearby
      ? [params.query, params.longitude, params.latitude, broad ? 12000 : 25000]
      : [params.query];
    const result = await db.query<CatalogRow>(
      `SELECT ${fields} FROM restaurant_catalog WHERE true ${location} ${name} ${filter} ${order} LIMIT 100`,
      values,
    );
    return result.rows.map(catalogRestaurant);
  }

  async function suggest(
    query: string,
    location?: { latitude: number; longitude: number },
  ): Promise<RestaurantSuggestion[]> {
    const result = await db.query<CatalogRow>(
      `SELECT ${fields} FROM restaurant_catalog WHERE name ILIKE '%' || $1 || '%' ORDER BY ${location ? 'location <-> ST_SetSRID(ST_MakePoint($2,$3),4326)::geography,' : ''} similarity(name,$1) DESC LIMIT 5`,
      location ? [query, location.longitude, location.latitude] : [query],
    );
    return result.rows.map((row) => ({
      id: row.id,
      label: [row.name, row.address].filter(Boolean).join(' · '),
      kind: 'restaurant' as const,
      query: row.chain_name || row.name,
      latitude: row.latitude,
      longitude: row.longitude,
    }));
  }
  return { ready, get, search, suggest };
}
