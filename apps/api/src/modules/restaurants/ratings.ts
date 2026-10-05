import { z } from 'zod';
import type { FastifyInstance } from 'fastify';
import type { Restaurant } from '@safeeat/contracts';
import type { Database } from '../../db/pool';
import type { Auth } from '../auth';

export function restaurantRatings(
  app: FastifyInstance,
  db: Database,
  auth: Auth,
  get: (id: string) => Promise<Restaurant>,
) {
  const path = '/api/v1/restaurants/:id/rating';
  const idSchema = z.object({ id: z.string().regex(/^[A-Za-z0-9_-]{1,200}$/) });
  app.get(path, async (request) => {
    const { id } = idSchema.parse(request.params);
    await get(id);
    const user = await auth.optionalUser(request);
    const result = await db.query<{ stars: number }>(
      'SELECT stars FROM restaurant_ratings WHERE restaurant_id=$1 AND user_id=$2',
      [id, user ?? null],
    );
    return { stars: result.rows[0]?.stars ?? null };
  });
  app.put(
    path,
    { preHandler: auth.protect, config: { rateLimit: { max: 30, timeWindow: '1 hour' } } },
    async (request) => {
      const { id } = idSchema.parse(request.params);
      const { stars } = z
        .object({ stars: z.number().int().min(1).max(5) })
        .strict()
        .parse(request.body);
      await get(id);
      await db.query(
        'INSERT INTO restaurant_ratings(restaurant_id,user_id,stars) VALUES($1,$2,$3) ON CONFLICT(restaurant_id,user_id) DO UPDATE SET stars=$3,updated_at=now()',
        [id, request.user.sub, stars],
      );
      return get(id);
    },
  );
  app.delete(path, { preHandler: auth.protect }, async (request) => {
    const { id } = idSchema.parse(request.params);
    await get(id);
    await db.query('DELETE FROM restaurant_ratings WHERE restaurant_id=$1 AND user_id=$2', [
      id,
      request.user.sub,
    ]);
    return get(id);
  });
}
