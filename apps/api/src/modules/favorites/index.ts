import { z } from 'zod';
import { randomUUID } from 'node:crypto';
import type { FastifyInstance } from 'fastify';
import type { Product } from '@safeeat/contracts';
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
  app.delete('/api/v1/saved', guarded, async (request, reply) => {
    z.object({ confirmation: z.literal('BORRAR') })
      .strict()
      .parse(request.body);
    const client = await db.connect();
    try {
      await client.query('BEGIN');
      await client.query('SELECT id FROM users WHERE id=$1 FOR UPDATE', [request.user.sub]);
      await client.query('DELETE FROM favorite_products WHERE user_id=$1', [request.user.sub]);
      await client.query('DELETE FROM saved_collections WHERE user_id=$1', [request.user.sub]);
      await client.query('DELETE FROM favorite_restaurants WHERE user_id=$1', [request.user.sub]);
      await client.query(
        "DELETE FROM shares WHERE kind='product' AND resource_id IN (SELECT id FROM products WHERE owner_id=$1)",
        [request.user.sub],
      );
      await client.query('DELETE FROM products WHERE owner_id=$1', [request.user.sub]);
      await client.query('COMMIT');
    } catch (error) {
      await client.query('ROLLBACK');
      throw error;
    } finally {
      client.release();
    }
    return reply.code(204).send();
  });
  const id = (params: unknown) => z.object({ id: z.string().min(1).max(200) }).parse(params).id;
  const collectionId = (params: unknown) =>
    z.object({ collectionId: z.string().uuid() }).parse(params).collectionId;
  app.get('/api/v1/collections', guarded, async (request) => {
    const result = await db.query<{ id: string; name: string; count: number }>(
      'SELECT c.id,c.name,count(cp.product_id)::int AS count FROM saved_collections c LEFT JOIN saved_collection_products cp ON cp.collection_id=c.id WHERE c.user_id=$1 GROUP BY c.id ORDER BY c.created_at DESC',
      [request.user.sub],
    );
    return { items: result.rows };
  });
  app.post('/api/v1/collections', guarded, async (request, reply) => {
    const { name } = z
      .object({ name: z.string().trim().min(1).max(60) })
      .strict()
      .parse(request.body);
    const collection = { id: randomUUID(), name, count: 0 };
    await db.query('INSERT INTO saved_collections(id,user_id,name) VALUES($1,$2,$3)', [
      collection.id,
      request.user.sub,
      name,
    ]);
    return reply.code(201).send(collection);
  });
  app.get('/api/v1/collections/for-product/:id', guarded, async (request) => {
    const result = await db.query<{ id: string }>(
      'SELECT c.id FROM saved_collections c JOIN saved_collection_products cp ON cp.collection_id=c.id WHERE c.user_id=$1 AND cp.product_id=$2',
      [request.user.sub, id(request.params)],
    );
    return { ids: result.rows.map((r) => r.id) };
  });
  app.delete('/api/v1/collections/:collectionId', guarded, async (request, reply) => {
    await db.query('DELETE FROM saved_collections WHERE id=$1 AND user_id=$2', [
      collectionId(request.params),
      request.user.sub,
    ]);
    return reply.code(204).send();
  });
  app.get('/api/v1/collections/:collectionId/products', guarded, async (request) => {
    const result = await db.query<{ data: Product; expired: boolean }>(
      'SELECT p.data,p.expires_at<now() AS expired FROM saved_collection_products cp JOIN saved_collections c ON c.id=cp.collection_id JOIN products p ON p.id=cp.product_id WHERE c.id=$1 AND c.user_id=$2 ORDER BY cp.added_at DESC LIMIT 100',
      [collectionId(request.params), request.user.sub],
    );
    return {
      items: await Promise.all(result.rows.map((r) => products.hydrateStored(r.data, r.expired))),
    };
  });
  app.get('/api/v1/collections/:collectionId/products/:id', guarded, async (request) => {
    const collection = collectionId(request.params),
      product = id(request.params);
    const result = await db.query(
      'SELECT 1 FROM saved_collection_products cp JOIN saved_collections c ON c.id=cp.collection_id WHERE c.id=$1 AND c.user_id=$2 AND cp.product_id=$3',
      [collection, request.user.sub, product],
    );
    return { saved: !!result.rowCount };
  });
  app.post('/api/v1/collections/:collectionId/products/:id', guarded, async (request, reply) => {
    const collection = collectionId(request.params),
      item = await products.get(id(request.params), request.user.sub);
    await db.query(
      'INSERT INTO saved_collection_products(collection_id,product_id) SELECT id,$3 FROM saved_collections WHERE id=$1 AND user_id=$2 ON CONFLICT DO NOTHING',
      [collection, request.user.sub, item.id],
    );
    await db.query(
      'INSERT INTO favorite_products(user_id,product_id) VALUES($1,$2) ON CONFLICT DO NOTHING',
      [request.user.sub, item.id],
    );
    return reply.code(204).send();
  });
  app.delete('/api/v1/collections/:collectionId/products/:id', guarded, async (request, reply) => {
    await db.query(
      'DELETE FROM saved_collection_products cp USING saved_collections c WHERE cp.collection_id=c.id AND c.id=$1 AND c.user_id=$2 AND cp.product_id=$3',
      [collectionId(request.params), request.user.sub, id(request.params)],
    );
    return reply.code(204).send();
  });
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
      items: await Promise.all(page.map((r) => products.hydrateStored(r.data, r.expired))),
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
    await db.query(
      'DELETE FROM saved_collection_products cp USING saved_collections c WHERE cp.collection_id=c.id AND c.user_id=$1 AND cp.product_id=$2',
      [request.user.sub, id(request.params)],
    );
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
