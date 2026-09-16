import { randomBytes } from 'node:crypto';
import { z } from 'zod';
import type { FastifyInstance } from 'fastify';
import { ShareSchema } from '@safeeat/contracts';
import type { Database } from '../../db/pool';
import type { Auth } from '../auth';
import type { Products } from '../products';
import type { Restaurants } from '../restaurants';
import { restaurantId } from '../restaurants';
import { notFound } from '../../plugins/errors';
export function sharingRoutes(
  app: FastifyInstance,
  db: Database,
  auth: Auth,
  products: Products,
  restaurants: Restaurants,
  publicUrl: string,
) {
  for (const kind of ['product', 'restaurant'] as const) {
    app.post(
      `/api/v1/shares/${kind}s/:id`,
      { config: { rateLimit: { max: 20, timeWindow: '1 minute' } } },
      async (request, reply) => {
        const { id } = z
          .object({ id: kind === 'restaurant' ? restaurantId : z.string().max(80) })
          .parse(request.params);
        const userId = await auth.optionalUser(request);
        const item =
          kind === 'product' ? await products.get(id, userId) : await restaurants.get(id);
        const token = randomBytes(24).toString('hex');
        const owner =
          kind === 'product' && 'userCreated' in item && item.userCreated ? userId : null;
        await db.query('INSERT INTO shares(token,kind,resource_id,owner_id) VALUES($1,$2,$3,$4)', [
          token,
          kind,
          id,
          owner ?? null,
        ]);
        return reply
          .code(201)
          .send({ token, url: `${publicUrl}/${kind === 'product' ? 'p' : 'r'}/${token}` });
      },
    );
  }
  app.get('/api/v1/public/shares/:token', async (request) => {
    const { token } = z.object({ token: z.string().regex(/^[a-f0-9]{48}$/) }).parse(request.params);
    const row = (
      await db.query<{
        kind: 'product' | 'restaurant';
        resource_id: string;
        owner_id: string | null;
      }>('SELECT kind,resource_id,owner_id FROM shares WHERE token=$1', [token])
    ).rows[0];
    if (!row) throw notFound();
    // Explicit public contract strips database ownership, sessions and other private fields.
    return ShareSchema.parse({
      type: row.kind,
      item:
        row.kind === 'product'
          ? await products.get(row.resource_id, row.owner_id ?? undefined)
          : await restaurants.get(row.resource_id),
    });
  });
}
