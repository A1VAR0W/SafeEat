import { z } from 'zod';
import type { FastifyInstance } from 'fastify';
import type { Database } from '../db/pool';
import type { Auth } from './auth';
import { ApiError, notFound } from '../plugins/errors';

// Table and column identifiers below are constants, never request input.
export function reviewRoutes(
  app: FastifyInstance,
  db: Database,
  auth: Auth,
  kind: 'products' | 'restaurants',
  resolve: (
    id: string,
    user?: string,
  ) => Promise<{ chain?: { key: string }; reviewGroup?: { key: string } }>,
) {
  const restaurant = kind === 'restaurants';
  const table = restaurant ? 'restaurant_gluten_reports' : 'product_gluten_reports';
  const resource = restaurant ? 'restaurant_id' : 'product_id';
  const status = restaurant ? 'support' : 'status';
  const scope = restaurant
    ? '(r.restaurant_id=$1 OR (r.chain_key=$2 AND r.chain_key IS NOT NULL))'
    : 'r.product_id=$1';
  app.get(`/api/v1/${kind}/:id/reviews`, async (request) => {
    const { id } = z.object({ id: z.string().max(200) }).parse(request.params);
    const user = await auth.optionalUser(request);
    const item = await resolve(id, user);
    const userParam = restaurant ? '$3' : '$2';
    const result = await db.query(
      `SELECT r.id,r.revision,COALESCE(r.user_id=${userParam}::uuid,false) AS own,
       COALESCE(bool_or(c.user_id=${userParam}::uuid),false) AS "confirmedByMe",r.note,r.${status} AS status,
       r.updated_at AS "updatedAt",COALESCE(min(c.confirmed_at),CASE WHEN a.authorized_at IS NOT NULL THEN GREATEST(r.updated_at,a.authorized_at) END) AS "verifiedAt",
       CASE WHEN a.authorized_at IS NOT NULL THEN 'authorized' WHEN min(c.confirmed_at) IS NOT NULL THEN 'manual' END AS "verificationMethod",
       count(c.user_id)::int AS confirmations,(r.${resource}<>$1) AS inherited
       FROM ${table} r LEFT JOIN review_confirmations c ON c.report_id=r.id AND c.revision=r.revision AND c.kind='${kind}'
       LEFT JOIN (SELECT user_id,max(assigned_at) AS authorized_at FROM user_roles WHERE role_code='authorized_celiac' GROUP BY user_id) a ON a.user_id=r.user_id
       WHERE ${scope} GROUP BY r.id,r.revision,r.user_id,r.note,r.${status},r.updated_at,r.${resource},a.authorized_at
       ORDER BY r.updated_at DESC LIMIT 100`,
      restaurant
        ? [id, item.chain?.key ?? item.reviewGroup?.key ?? null, user ?? null]
        : [id, user ?? null],
    );
    return { items: result.rows };
  });
  app.post(
    `/api/v1/${kind}/:id/reviews/:reviewId/confirm`,
    {
      preHandler: auth.protect,
      config: { rateLimit: { max: 30, timeWindow: '1 hour' } },
    },
    async (request) => {
      const { id, reviewId } = z
        .object({ id: z.string().max(200), reviewId: z.string().uuid() })
        .parse(request.params);
      const { revision } = z.object({ revision: z.string().uuid() }).strict().parse(request.body);
      const item = await resolve(id, request.user.sub);
      const client = await db.connect();
      try {
        await client.query('BEGIN');
        const result = await client.query<{ user_id: string; revision: string }>(
          `SELECT r.user_id,r.revision FROM ${table} r WHERE ${scope} AND r.id=$${restaurant ? 3 : 2} FOR UPDATE`,
          restaurant
            ? [id, item.chain?.key ?? item.reviewGroup?.key ?? null, reviewId]
            : [id, reviewId],
        );
        const report = result.rows[0];
        if (!report) throw notFound();
        if (report.user_id === request.user.sub)
          throw new ApiError(403, 'SELF_CONFIRMATION', 'Otra persona debe confirmar tu revisión.');
        if (report.revision !== revision)
          throw new ApiError(
            409,
            'REVIEW_CHANGED',
            'El motivo ha cambiado. Vuelve a leerlo antes de confirmarlo.',
          );
        await client.query(
          'INSERT INTO review_confirmations(kind,report_id,revision,user_id) VALUES($1,$2,$3,$4) ON CONFLICT DO NOTHING',
          [kind, reviewId, revision, request.user.sub],
        );
        await client.query('COMMIT');
        return { confirmed: true };
      } catch (error) {
        await client.query('ROLLBACK');
        throw error;
      } finally {
        client.release();
      }
    },
  );
}
