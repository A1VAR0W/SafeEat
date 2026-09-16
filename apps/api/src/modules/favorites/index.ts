import { z } from 'zod';
import type { FastifyInstance } from 'fastify';
import { ProductSchema, type Product } from '@safeeat/contracts';
import type { Database } from '../../db/pool';
import type { Auth } from '../auth';
import type { Products } from '../products';
import type { Restaurants } from '../restaurants';
import { restaurantId } from '../restaurants';
export function favoriteRoutes(
  app: FastifyInstance,
  db: Database,
  auth: Auth,
  products: Products,
  restaurants: Restaurants,
) {
  const guarded = { preHandler: auth.protect };
  const id = (params: unknown) => z.object({ id: z.string().min(1).max(200) }).parse(params).id;
  for (const kind of ['products', 'restaurants'] as const) {
    app.get(`/api/v1/favorites/${kind}/:id`, guarded, async (request) => {
      const sql =
        kind === 'products'
          ? 'SELECT 1 FROM favorite_products WHERE user_id=$1 AND product_id=$2'
          : 'SELECT 1 FROM favorite_restaurants WHERE user_id=$1 AND restaurant_id=$2';
      const result = await db.query(sql, [request.user.sub, id(request.params)]);
      return { saved: !!result.rowCount };
    });
  }
  app.get('/api/v1/favorites/products', guarded, async (request) => {
    const { cursor } = z.object({ cursor: z.string().max(200).optional() }).parse(request.query);
    const rows = await db.query<{ data: Product; id: string; expired: boolean }>(
      'SELECT p.data,p.id,p.expires_at<now() AS expired FROM favorite_products f JOIN products p ON p.id=f.product_id WHERE f.user_id=$1 AND ($2::text IS NULL OR p.id>$2) ORDER BY p.id LIMIT 51',
      [request.user.sub, cursor ?? null],
    );
    const page = rows.rows.slice(0, 50);
    return {
      items: page.map((r) => ({ ...ProductSchema.parse(r.data), stale: r.expired })),
      nextCursor: rows.rows.length > 50 ? page.at(-1)!.id : null,
    };
  });
  app.post('/api/v1/favorites/products/:id', guarded, async (request, reply) => {
    const item = await products.get(id(request.params), request.user.sub);
    await db.query(
      'INSERT INTO favorite_products(user_id,product_id) VALUES($1,$2) ON CONFLICT DO NOTHING',
      [request.user.sub, item.id],
    );
    return reply.code(204).send();
  });
  app.delete('/api/v1/favorites/products/:id', guarded, async (request, reply) => {
    await db.query('DELETE FROM favorite_products WHERE user_id=$1 AND product_id=$2', [
      request.user.sub,
      id(request.params),
    ]);
    return reply.code(204).send();
  });
  app.get('/api/v1/favorites/restaurants', guarded, async (request) => {
    const { cursor } = z.object({ cursor: restaurantId.optional() }).parse(request.query);
    const rows = await db.query<{ restaurant_id: string }>(
      'SELECT restaurant_id FROM favorite_restaurants WHERE user_id=$1 AND ($2::text IS NULL OR restaurant_id>$2) ORDER BY restaurant_id LIMIT 51',
      [request.user.sub, cursor ?? null],
    );
    return {
      items: rows.rows.slice(0, 50).map((r) => ({ id: r.restaurant_id })),
      nextCursor: rows.rows.length > 50 ? rows.rows[49]!.restaurant_id : null,
    };
  });
  app.post('/api/v1/favorites/restaurants/:id', guarded, async (request, reply) => {
    const item = await restaurants.get(restaurantId.parse(id(request.params)));
    await db.query(
      'INSERT INTO favorite_restaurants(user_id,restaurant_id) VALUES($1,$2) ON CONFLICT DO NOTHING',
      [request.user.sub, item.id],
    );
    return reply.code(204).send();
  });
  app.delete('/api/v1/favorites/restaurants/:id', guarded, async (request, reply) => {
    await db.query('DELETE FROM favorite_restaurants WHERE user_id=$1 AND restaurant_id=$2', [
      request.user.sub,
      id(request.params),
    ]);
    return reply.code(204).send();
  });
}
