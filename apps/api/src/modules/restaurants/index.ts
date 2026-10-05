import { randomUUID } from 'node:crypto';
import { z } from 'zod';
import type { FastifyInstance } from 'fastify';
import { RestaurantSchema, type Restaurant } from '@safeeat/contracts';
import type { Database } from '../../db/pool';
import type { Auth } from '../auth';
import type { RestaurantProvider, RestaurantSearch } from '../../providers/types';
import { ApiError, notFound } from '../../plugins/errors';
import { restaurantCatalog } from './catalog';
import { reviewRoutes } from '../reviews';
import { combineSuggestions } from './suggestions';
import { restaurantRatings } from './ratings';
import { matchesRestaurantSearch, restaurantSearchName } from './search';

export const restaurantId = z.string().regex(/^[A-Za-z0-9_-]{1,200}$/);
const support = z.enum(['DEDICATED_GLUTEN_FREE', 'GLUTEN_FREE_OPTIONS', 'NO_GLUTEN_FREE_OPTIONS']);
type CommunityRow = {
  id: string;
  name: string;
  address: string;
  latitude: number;
  longitude: number;
  created_at: Date;
};
function community(row: CommunityRow): Restaurant {
  return RestaurantSchema.parse({
    id: row.id,
    name: row.name,
    address: row.address,
    latitude: row.latitude,
    longitude: row.longitude,
    mapsUrl: `https://www.google.com/maps/search/?api=1&query=${row.latitude},${row.longitude}`,
    glutenSupport: 'UNKNOWN',
    evidence: [],
    source: 'Comunidad SafeEat',
    communityCreated: true,
    fetchedAt: row.created_at.toISOString(),
  });
}

export function restaurantService(
  app: FastifyInstance,
  db: Database,
  auth: Auth,
  provider: RestaurantProvider,
) {
  const catalog = restaurantCatalog(db);
  async function withReports(items: Restaurant[]): Promise<Restaurant[]> {
    if (!items.length) return items;
    const [aliases, ratings, groups] = await Promise.all([
      db.query<{ id: string; chain_key: string; chain_name: string }>(
        `SELECT target.id,a.chain_key,a.chain_name FROM unnest($1::text[],$2::text[]) AS target(id,name)
         JOIN restaurant_chain_aliases a ON a.alias=restaurant_brand_name(regexp_replace(target.name,'\\s+[-–·]\\s+.*$',''))`,
        [
          items.filter((item) => !item.chain).map((item) => item.id),
          items.filter((item) => !item.chain).map((item) => item.name),
        ],
      ),
      db.query<{ restaurant_id: string; average: number; count: number }>(
        'SELECT restaurant_id,avg(stars)::float8 AS average,count(*)::int AS count FROM restaurant_ratings WHERE restaurant_id=ANY($1::text[]) GROUP BY restaurant_id',
        [items.map((item) => item.id)],
      ),
      db.query<{ name_key: string; name: string; count: number }>(
        `SELECT name_key,min(name) AS name,count(DISTINCT id)::int AS count FROM (
           SELECT id,name,restaurant_brand_name(name) AS name_key FROM restaurant_catalog WHERE restaurant_brand_name(name)=ANY($1::text[]) AND restaurant_brand_name(name)<>''
           UNION ALL SELECT id,name,restaurant_brand_name(name) AS name_key FROM community_restaurants WHERE restaurant_brand_name(name)=ANY($1::text[]) AND restaurant_brand_name(name)<>''
         ) matches GROUP BY name_key HAVING count(DISTINCT id)>1`,
        [items.map((item) => restaurantSearchName(item.name))],
      ),
    ]);
    const chains = new Map(
      aliases.rows.map((row) => [row.id, { key: row.chain_key, name: row.chain_name }]),
    );
    const scores = new Map(
      ratings.rows.map((row) => [row.restaurant_id, { average: row.average, count: row.count }]),
    );
    const sameNames = new Map(
      groups.rows.map((row) => [
        row.name_key,
        { key: `name:${row.name_key}`, name: row.name, count: row.count },
      ]),
    );
    items = items.map((item) => ({
      ...item,
      chain: item.chain ?? chains.get(item.id),
      rating: scores.get(item.id),
      reviewGroup: sameNames.get(restaurantSearchName(item.name)),
    }));
    const reports = await db.query<{
      restaurant_id: string;
      dedicated: number;
      options: number;
      no_options: number;
    }>(
      `WITH relevant AS (
         SELECT DISTINCT ON (target.id,r.user_id) target.id AS restaurant_id,r.support
         FROM unnest($1::text[],$2::text[]) AS target(id,chain_key)
         JOIN restaurant_gluten_reports r ON r.restaurant_id=target.id OR (r.chain_key=target.chain_key AND r.chain_key IS NOT NULL)
         ORDER BY target.id,r.user_id,(r.restaurant_id=target.id) DESC,r.updated_at DESC,r.id
       )
       SELECT restaurant_id,count(*) FILTER (WHERE support='DEDICATED_GLUTEN_FREE')::int AS dedicated,
       count(*) FILTER (WHERE support='GLUTEN_FREE_OPTIONS')::int AS options,
       count(*) FILTER (WHERE support='NO_GLUTEN_FREE_OPTIONS')::int AS no_options
       FROM relevant GROUP BY restaurant_id`,
      [
        items.map((item) => item.id),
        items.map((item) => item.chain?.key ?? item.reviewGroup?.key ?? null),
      ],
    );
    const byId = new Map(reports.rows.map((report) => [report.restaurant_id, report]));
    return items.map((item) => {
      const report = byId.get(item.id);
      if (!report) return item;
      const positive = report.dedicated + report.options;
      const fromUsers =
        report.no_options && positive
          ? 'UNKNOWN'
          : report.no_options
            ? 'NO_GLUTEN_FREE_OPTIONS'
            : report.dedicated && !report.options
              ? 'DEDICATED_GLUTEN_FREE'
              : positive
                ? 'GLUTEN_FREE_OPTIONS'
                : 'UNKNOWN';
      const conflict =
        (!!report.no_options && !!positive) ||
        (item.glutenSupport === 'NO_GLUTEN_FREE_OPTIONS' && positive > 0) ||
        (item.glutenSupport !== 'UNKNOWN' &&
          item.glutenSupport !== 'NO_GLUTEN_FREE_OPTIONS' &&
          report.no_options > 0);
      return RestaurantSchema.parse({
        ...item,
        glutenSupport: conflict
          ? 'UNKNOWN'
          : fromUsers === 'UNKNOWN'
            ? item.glutenSupport
            : fromUsers,
        evidence: [
          ...(item.glutenSupport === 'UNKNOWN' ? [] : item.evidence),
          {
            source: 'Usuarios de SafeEat',
            kind: 'uncertainty',
            text: conflict
              ? 'Hay indicaciones contradictorias sobre las opciones sin gluten.'
              : `${report.dedicated + report.options + report.no_options} usuario(s) han compartido su experiencia. Confirma el menú y la preparación.`,
          },
        ],
        glutenReports: {
          dedicated: report.dedicated,
          options: report.options,
          noOptions: report.no_options,
        },
      });
    });
  }

  async function get(id: string): Promise<Restaurant> {
    if (id.startsWith('se_')) {
      const result = await db.query<CommunityRow>(
        'SELECT id,name,address,latitude,longitude,created_at FROM community_restaurants WHERE id=$1',
        [id],
      );
      if (!result.rows[0]) throw notFound();
      return (await withReports([community(result.rows[0])]))[0]!;
    }
    const item = id.startsWith('osm_')
      ? ((await catalog.get(id)) ?? (await provider.getById(id)))
      : await provider.getById(id);
    if (!item) throw notFound();
    return (await withReports([item]))[0]!;
  }

  async function search(params: RestaurantSearch): Promise<Restaurant[]> {
    const nearby = params.latitude !== undefined && params.longitude !== undefined;
    const communityRows = await db.query<CommunityRow>(
      nearby
        ? 'SELECT id,name,address,latitude,longitude,created_at FROM community_restaurants WHERE latitude BETWEEN $1 AND $2 AND longitude BETWEEN $3 AND $4 AND ($5::boolean OR position(restaurant_brand_name($6) in restaurant_brand_name(name))>0) ORDER BY created_at DESC LIMIT 20'
        : 'SELECT id,name,address,latitude,longitude,created_at FROM community_restaurants WHERE position(restaurant_brand_name($1) in restaurant_brand_name(name))>0 ORDER BY created_at DESC LIMIT 20',
      nearby
        ? [
            params.latitude! - 0.025,
            params.latitude! + 0.025,
            params.longitude! - 0.035,
            params.longitude! + 0.035,
            /^(restaurantes?|cerca de m[ií])$/i.test(params.query),
            params.query,
          ]
        : [params.query],
    );
    const local = communityRows.rows.map(community);
    let external: Restaurant[];
    try {
      const fallback = {
        ...params,
        glutenFree: params.glutenFree || params.glutenMode !== undefined,
      };
      external = (await catalog.ready()) ? await catalog.search(params) : [];
      if (!external.length) external = await provider.search(fallback);
    } catch (error) {
      if (!local.length) throw error;
      external = [];
    }
    const alias = await db.query<{ chain_key: string }>(
      'SELECT chain_key FROM restaurant_chain_aliases WHERE alias=restaurant_brand_name($1)',
      [params.query],
    );
    const merged = (await withReports([...local, ...external])).filter((item) =>
      matchesRestaurantSearch(item, params.query, alias.rows[0]?.chain_key),
    );
    return params.glutenMode === 'dedicated'
      ? merged.filter((item) => item.glutenSupport === 'DEDICATED_GLUTEN_FREE')
      : params.glutenFree || params.glutenMode === 'options'
        ? merged.filter((item) =>
            ['DEDICATED_GLUTEN_FREE', 'GLUTEN_FREE_OPTIONS'].includes(item.glutenSupport),
          )
        : merged;
  }

  app.get(
    '/api/v1/restaurants/suggest',
    { config: { rateLimit: { max: 60, timeWindow: '1 minute' } } },
    async (request, reply) => {
      const { q, latitude, longitude, localOnly } = z
        .object({
          q: z.string().trim().min(2).max(120),
          latitude: z.coerce.number().min(-90).max(90).optional(),
          longitude: z.coerce.number().min(-180).max(180).optional(),
          localOnly: z
            .enum(['true', 'false'])
            .transform((value) => value === 'true')
            .optional(),
        })
        .refine((value) => (value.latitude === undefined) === (value.longitude === undefined))
        .parse(request.query);
      const location =
        latitude !== undefined && longitude !== undefined ? { latitude, longitude } : undefined;
      const disconnected = new AbortController();
      const close = () => {
        if (!reply.raw.writableFinished) disconnected.abort();
      };
      reply.raw.once('close', close);
      try {
        const [local, indexed, external] = await Promise.all([
          db.query<{ id: string; name: string; latitude: number; longitude: number }>(
            'SELECT id,name,latitude,longitude FROM community_restaurants WHERE name ILIKE $1 ORDER BY created_at DESC LIMIT 4',
            [`%${q.replace(/[\\%_]/g, '\\$&')}%`],
          ),
          catalog.suggest(q, location),
          !localOnly && provider.suggest
            ? provider
                .suggest(
                  q,
                  location,
                  // Local suggestions arrive in a separate fast request. Allow the
                  // enrichment request enough time to return real cities/streets.
                  AbortSignal.any([disconnected.signal, AbortSignal.timeout(6500)]),
                )
                .catch(() => [])
            : Promise.resolve([]),
        ]);
        return {
          items: combineSuggestions(
            [
              ...local.rows.map((row) => ({
                id: row.id,
                label: row.name,
                kind: 'restaurant' as const,
                query: row.name,
                latitude: row.latitude,
                longitude: row.longitude,
              })),
              ...indexed,
            ],
            external,
          ),
        };
      } finally {
        reply.raw.off('close', close);
      }
    },
  );
  app.get(
    '/api/v1/restaurants/search',
    { config: { rateLimit: { max: 15, timeWindow: '1 minute' } } },
    async (request) => {
      const q = z
        .object({
          q: z.string().trim().min(2).max(120),
          latitude: z.coerce.number().min(-90).max(90).optional(),
          longitude: z.coerce.number().min(-180).max(180).optional(),
          glutenFree: z
            .enum(['true', 'false'])
            .transform((value) => value === 'true')
            .optional(),
          glutenMode: z.enum(['dedicated', 'options']).optional(),
        })
        .refine((value) => (value.latitude === undefined) === (value.longitude === undefined))
        .parse(request.query);
      return {
        items: await search({
          query: q.q,
          latitude: q.latitude,
          longitude: q.longitude,
          glutenFree: q.glutenFree,
          glutenMode: q.glutenMode,
        }),
      };
    },
  );
  app.post(
    '/api/v1/restaurants',
    { preHandler: auth.protect, config: { rateLimit: { max: 4, timeWindow: '1 day' } } },
    async (request, reply) => {
      const body = z
        .object({
          name: z.string().trim().min(2).max(160),
          address: z.string().trim().min(5).max(300),
          latitude: z.number().min(-90).max(90),
          longitude: z.number().min(-180).max(180),
          support,
          note: z.string().trim().min(10).max(500),
        })
        .strict()
        .parse(request.body);
      const id = `se_${randomUUID().replace(/-/g, '')}`;
      const duplicate = await db.query(
        'SELECT 1 FROM community_restaurants WHERE lower(name)=lower($1) AND abs(latitude-$2)<0.0005 AND abs(longitude-$3)<0.0005 LIMIT 1',
        [body.name, body.latitude, body.longitude],
      );
      if (duplicate.rowCount)
        return reply.code(409).send({
          error: {
            code: 'RESTAURANT_EXISTS',
            message: 'Ese restaurante ya aparece cerca de aquí.',
          },
        });
      const client = await db.connect();
      try {
        await client.query('BEGIN');
        await client.query(
          'INSERT INTO community_restaurants(id,user_id,name,address,latitude,longitude) VALUES($1,$2,$3,$4,$5,$6)',
          [id, request.user.sub, body.name, body.address, body.latitude, body.longitude],
        );
        await client.query(
          'INSERT INTO restaurant_gluten_reports(restaurant_id,user_id,support,note) VALUES($1,$2,$3,$4)',
          [id, request.user.sub, body.support, body.note],
        );
        await client.query('COMMIT');
      } catch (error) {
        await client.query('ROLLBACK');
        throw error;
      } finally {
        client.release();
      }
      return reply.code(201).send(await get(id));
    },
  );
  app.post(
    '/api/v1/restaurants/:id/gluten-report',
    { preHandler: auth.protect, config: { rateLimit: { max: 8, timeWindow: '1 hour' } } },
    async (request) => {
      const id = z.object({ id: restaurantId }).parse(request.params).id;
      const body = z
        .object({
          support,
          note: z.string().trim().min(10).max(500),
          scope: z.enum(['local', 'chain']).optional(),
        })
        .strict()
        .parse(request.body);
      const item = await get(id);
      const scope =
        body.scope ?? (body.support !== 'NO_GLUTEN_FREE_OPTIONS' && item.chain ? 'chain' : 'local');
      const chainKey = item.chain?.key ?? item.reviewGroup?.key;
      if (scope === 'chain' && !chainKey)
        throw new ApiError(
          400,
          'CHAIN_NOT_AVAILABLE',
          'Todavía no encontramos otros locales de este restaurante.',
        );
      await db.query(
        'INSERT INTO restaurant_gluten_reports(restaurant_id,user_id,support,note,chain_key,scope) VALUES($1,$2,$3,$4,$5,$6) ON CONFLICT(restaurant_id,user_id) DO UPDATE SET support=$3,note=$4,chain_key=$5,scope=$6,updated_at=now()',
        [id, request.user.sub, body.support, body.note, scope === 'chain' ? chainKey : null, scope],
      );
      return get(id);
    },
  );
  app.get('/api/v1/restaurants/:id', async (request) =>
    get(z.object({ id: restaurantId }).parse(request.params).id),
  );
  reviewRoutes(app, db, auth, 'restaurants', get);
  restaurantRatings(app, db, auth, get);
  return { get };
}
export type Restaurants = ReturnType<typeof restaurantService>;
