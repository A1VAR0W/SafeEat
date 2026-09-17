import { z } from 'zod';
import type { FastifyInstance } from 'fastify';
import type { RestaurantProvider } from '../../providers/types';
import { notFound } from '../../plugins/errors';
export const restaurantId = z.string().regex(/^[A-Za-z0-9_-]{1,200}$/);
export function restaurantService(app: FastifyInstance, provider: RestaurantProvider) {
  const get = async (id: string) => {
    const item = await provider.getById(id);
    if (!item) throw notFound();
    return item;
  };
  app.get(
    '/api/v1/restaurants/suggest',
    { config: { rateLimit: { max: 20, timeWindow: '1 minute' } } },
    async (request) => {
      const { q } = z.object({ q: z.string().trim().min(2).max(120) }).parse(request.query);
      return { items: provider.suggest ? await provider.suggest(q) : [] };
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
        })
        .refine((v) => (v.latitude === undefined) === (v.longitude === undefined))
        .parse(request.query);
      return {
        items: await provider.search({ query: q.q, latitude: q.latitude, longitude: q.longitude }),
      };
    },
  );
  app.get('/api/v1/restaurants/:id', async (request) =>
    get(z.object({ id: restaurantId }).parse(request.params).id),
  );
  return { get };
}
export type Restaurants = ReturnType<typeof restaurantService>;
