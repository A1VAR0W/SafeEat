import { z } from 'zod';
import type { PoolClient } from 'pg';
import type { FastifyInstance, FastifyRequest } from 'fastify';
import type { Database } from '../db/pool';
import type { Auth } from './auth';
import { ApiError, notFound, unauthorized } from '../plugins/errors';

export function adminRoutes(app: FastifyInstance, db: Database, auth: Auth) {
  async function admin(request: FastifyRequest) {
    await auth.protect(request);
    if (
      !(
        await db.query("SELECT 1 FROM user_roles WHERE user_id=$1 AND role_code='ADMIN'", [
          request.user.sub,
        ])
      ).rowCount
    )
      throw new ApiError(
        403,
        'ADMIN_REQUIRED',
        'Solo los administradores pueden acceder a este apartado.',
      );
  }
  async function mutate(request: FastifyRequest, work: (client: PoolClient) => Promise<unknown>) {
    const client = await db.connect();
    try {
      await client.query('BEGIN');
      await client.query('SELECT pg_advisory_xact_lock(hashtextextended($1,0))', [
        `user:${request.user.sub}`,
      ]);
      if (
        !(
          await client.query(
            'SELECT 1 FROM refresh_sessions WHERE id=$1 AND user_id=$2 AND revoked_at IS NULL AND expires_at>now() FOR SHARE',
            [request.user.sid, request.user.sub],
          )
        ).rowCount
      )
        throw unauthorized();
      if (
        !(
          await client.query(
            "SELECT 1 FROM user_roles WHERE user_id=$1 AND role_code='ADMIN' FOR SHARE",
            [request.user.sub],
          )
        ).rowCount
      )
        throw new ApiError(403, 'ADMIN_REQUIRED', 'Ya no tienes permisos de administración.');
      const result = await work(client);
      await client.query('COMMIT');
      return result;
    } catch (error) {
      await client.query('ROLLBACK');
      throw error;
    } finally {
      client.release();
    }
  }
  const params = z.object({ id: z.string().uuid() });
  const config = { preHandler: admin, config: { rateLimit: { max: 60, timeWindow: '1 minute' } } };
  app.get('/api/v1/admin/users', config, async (request) => {
    const { q = '', after } = z
      .object({ q: z.string().trim().max(120).optional(), after: z.string().uuid().optional() })
      .parse(request.query);
    const result = await db.query(
      `SELECT u.id,u.email,u.first_name AS "firstName",u.last_name AS "lastName",
        COALESCE((SELECT array_agg(role_code ORDER BY role_code) FROM user_roles WHERE user_id=u.id),'{}') AS roles
       FROM users u WHERE ($1='' OR position(lower($1) in lower(concat_ws(' ',u.email,u.first_name,u.last_name)))>0
       OR (restaurant_brand_name($1)<>'' AND position(restaurant_brand_name($1) in restaurant_brand_name(concat_ws(' ',u.email,u.first_name,u.last_name)))>0))
       AND ($2::uuid IS NULL OR u.id>$2) ORDER BY u.id LIMIT 21`,
      [q, after ?? null],
    );
    return {
      items: result.rows.slice(0, 20),
      nextCursor: result.rows.length > 20 ? result.rows[19].id : null,
    };
  });
  for (const method of ['PUT', 'DELETE'] as const)
    app.route({
      method,
      url: '/api/v1/admin/users/:id/authorized',
      ...config,
      handler: async (request) => {
        const { id } = params.parse(request.params);
        return mutate(request, async (client) => {
          if (!(await client.query('SELECT 1 FROM users WHERE id=$1', [id])).rowCount)
            throw notFound();
          const changed =
            method === 'PUT'
              ? await client.query(
                  "INSERT INTO user_roles(user_id,role_code) VALUES($1,'authorized_celiac') ON CONFLICT DO NOTHING",
                  [id],
                )
              : await client.query(
                  "DELETE FROM user_roles WHERE user_id=$1 AND role_code='authorized_celiac'",
                  [id],
                );
          if (changed.rowCount)
            await client.query(
              'INSERT INTO admin_audit_log(actor_id,action,target_id) VALUES($1,$2,$3)',
              [
                request.user.sub,
                method === 'PUT' ? 'authorized_granted' : 'authorized_removed',
                id,
              ],
            );
          return { authorized: method === 'PUT' };
        });
      },
    });
  const kind = z.enum(['products', 'restaurants']);
  app.get('/api/v1/admin/reviews', config, async (request) => {
    const {
      kind: k,
      state = 'active',
      q = '',
      after,
    } = z
      .object({
        kind,
        state: z.enum(['active', 'revoked']).optional(),
        q: z.string().trim().max(120).optional(),
        after: z.string().uuid().optional(),
      })
      .parse(request.query);
    const restaurant = k === 'restaurants';
    const table = restaurant ? 'restaurant_gluten_reports' : 'product_gluten_reports';
    const resource = restaurant ? 'restaurant_id' : 'product_id';
    const status = restaurant ? 'support' : 'status';
    const result = await db.query(
      `SELECT r.id,r.revision,r.${resource} AS "resourceId",r.${status} AS status,r.note,
        r.updated_at AS "updatedAt",r.revoked_at AS "revokedAt",r.revocation_reason AS "revocationReason",
        u.id AS "authorId",u.email AS "authorEmail",concat_ws(' ',u.first_name,u.last_name) AS "authorName",
        COALESCE(manual.confirmed_at,CASE WHEN authorized.authorized_at IS NOT NULL THEN GREATEST(r.updated_at,authorized.authorized_at) END) AS "verifiedAt",
        CASE WHEN authorized.authorized_at IS NOT NULL THEN 'authorized' WHEN manual.confirmed_at IS NOT NULL THEN 'manual' END AS "verificationMethod",
        COALESCE(manual.confirmations,0)::int AS confirmations,
        ${restaurant ? 'COALESCE(c.name,community.name,r.restaurant_id)' : "COALESCE(p.data->>'name',r.product_id)"} AS "resourceName",
        ${restaurant ? 'r.scope' : "'local'"} AS scope
       FROM ${table} r JOIN users u ON u.id=r.user_id
       ${restaurant ? 'LEFT JOIN restaurant_catalog c ON c.id=r.restaurant_id LEFT JOIN community_restaurants community ON community.id=r.restaurant_id' : 'LEFT JOIN products p ON p.id=r.product_id'}
       LEFT JOIN (SELECT user_id,max(assigned_at) AS authorized_at FROM user_roles WHERE role_code='authorized_celiac' GROUP BY user_id) authorized ON authorized.user_id=r.user_id
       LEFT JOIN LATERAL (SELECT min(confirmed_at) AS confirmed_at,count(*)::int AS confirmations FROM review_confirmations WHERE kind='${k}' AND report_id=r.id AND revision=r.revision) manual ON true
       WHERE ${state === 'revoked' ? 'r.revoked_at IS NOT NULL' : 'r.revoked_at IS NULL'}
       AND ($1='' OR position(lower($1) in lower(concat_ws(' ',u.email,u.first_name,r.note,${restaurant ? 'c.name,community.name,r.restaurant_id' : "p.data->>'name',r.product_id"})))>0
       OR (restaurant_brand_name($1)<>'' AND position(restaurant_brand_name($1) in restaurant_brand_name(concat_ws(' ',u.email,u.first_name,r.note,${restaurant ? 'c.name,community.name,r.restaurant_id' : "p.data->>'name',r.product_id"})))>0))
       AND ($2::uuid IS NULL OR r.id>$2) ORDER BY r.id LIMIT 21`,
      [q, after ?? null],
    );
    return {
      items: result.rows.slice(0, 20),
      nextCursor: result.rows.length > 20 ? result.rows[19].id : null,
    };
  });
  app.post('/api/v1/admin/reviews/products/:id/verify', config, async (request) => {
    const { id } = params.parse(request.params);
    const { revision } = z.object({ revision: z.string().uuid() }).strict().parse(request.body);
    return mutate(request, async (client) => {
      const found = await client.query<{
        revision: string;
        user_id: string;
        revoked_at: Date | null;
      }>('SELECT revision,user_id,revoked_at FROM product_gluten_reports WHERE id=$1 FOR UPDATE', [
        id,
      ]);
      const row = found.rows[0];
      if (!row) throw notFound();
      if (row.revision !== revision)
        throw new ApiError(
          409,
          'REVIEW_CHANGED',
          'La revisión ha cambiado. Vuelve a leerla antes de continuar.',
        );
      if (row.revoked_at)
        throw new ApiError(409, 'REVIEW_REVOKED', 'No se puede verificar una revisión revocada.');
      if (row.user_id === request.user.sub)
        throw new ApiError(403, 'SELF_CONFIRMATION', 'No puedes verificar tu propia revisión.');
      const inserted = await client.query(
        "INSERT INTO review_confirmations(kind,report_id,revision,user_id) VALUES('products',$1,$2,$3) ON CONFLICT DO NOTHING",
        [id, revision, request.user.sub],
      );
      if (inserted.rowCount)
        await client.query(
          "INSERT INTO admin_audit_log(actor_id,action,target_id,details) VALUES($1,'product_review_verified',$2,$3)",
          [request.user.sub, id, JSON.stringify({ revision })],
        );
      return { verified: true, alreadyVerifiedByAdmin: !inserted.rowCount };
    });
  });
  app.post('/api/v1/admin/reviews/:kind/:id/moderate', config, async (request) => {
    const { kind: k, id } = params.extend({ kind }).parse(request.params);
    const { revision, action, reason } = z
      .object({
        revision: z.string().uuid(),
        action: z.enum(['revoke', 'restore']),
        reason: z.string().trim().min(5).max(500),
      })
      .strict()
      .parse(request.body);
    const table = k === 'restaurants' ? 'restaurant_gluten_reports' : 'product_gluten_reports';
    return mutate(request, async (client) => {
      const found = await client.query<{ revision: string; revoked_at: Date | null }>(
        `SELECT revision,revoked_at FROM ${table} WHERE id=$1 FOR UPDATE`,
        [id],
      );
      const row = found.rows[0];
      if (!row) throw notFound();
      if (row.revision !== revision)
        throw new ApiError(
          409,
          'REVIEW_CHANGED',
          'La revisión ha cambiado. Vuelve a leerla antes de continuar.',
        );
      if ((action === 'revoke') === !!row.revoked_at)
        throw new ApiError(
          409,
          'MODERATION_CHANGED',
          'El estado de la revisión ha cambiado. Actualiza la lista.',
        );
      const updated = await client.query(
        `UPDATE ${table} SET revoked_at=CASE WHEN $4::boolean THEN now() ELSE NULL END,revoked_by=CASE WHEN $4 THEN $2::uuid ELSE NULL END,revocation_reason=$3 WHERE id=$1 RETURNING revision`,
        [id, request.user.sub, reason, action === 'revoke'],
      );
      await client.query(
        'INSERT INTO admin_audit_log(actor_id,action,target_id,details) VALUES($1,$2,$3,$4)',
        [
          request.user.sub,
          action === 'revoke' ? 'review_revoked' : 'review_restored',
          id,
          JSON.stringify({ kind: k, reason, previousRevision: revision }),
        ],
      );
      return { revoked: action === 'revoke', revision: updated.rows[0].revision };
    });
  });
}
