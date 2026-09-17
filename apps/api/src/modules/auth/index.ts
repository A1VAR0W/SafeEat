import { createHmac, randomBytes, randomUUID } from 'node:crypto';
import argon2 from 'argon2';
import type { PoolClient } from 'pg';
import type { FastifyInstance, FastifyRequest } from 'fastify';
import {
  registrationSchema,
  loginSchema,
  refreshSchema,
  emailChangeSchema,
  passwordChangeSchema,
  deleteAccountSchema,
  profileFieldsSchema,
  UserSchema,
} from '@safeeat/contracts';
import type { Database } from '../../db/pool';
import type { Env } from '../../config/env';
import { ApiError, unauthorized } from '../../plugins/errors';
declare module '@fastify/jwt' {
  interface FastifyJWT {
    payload: { sub: string; sid: string };
    user: { sub: string; sid: string };
  }
}
type UserRow = {
  id: string;
  email: string;
  password_hash: string;
  first_name: string | null;
  last_name: string | null;
  date_of_birth: string | null;
  product_updates_consent: boolean;
  product_updates_consent_at: Date | null;
  product_updates_consent_version: string | null;
};
type SessionRow = {
  id: string;
  user_id: string;
  family_id: string;
  expires_at: Date;
  revoked_at: Date | null;
};
const columns =
  'id,email,password_hash,first_name,last_name,date_of_birth::text,product_updates_consent,product_updates_consent_at,product_updates_consent_version';
const hashPassword = (password: string) =>
  argon2.hash(password, { type: argon2.argon2id, memoryCost: 19456, timeCost: 2, parallelism: 1 });
function publicUser(user: UserRow) {
  return UserSchema.parse({
    id: user.id,
    email: user.email,
    firstName: user.first_name,
    lastName: user.last_name,
    dateOfBirth: user.date_of_birth,
    productUpdatesConsent: user.product_updates_consent,
    productUpdatesConsentAt: user.product_updates_consent_at?.toISOString() ?? null,
    productUpdatesConsentVersion: user.product_updates_consent_version,
  });
}
export function authService(app: FastifyInstance, db: Database, env: Env) {
  const hash = (token: string) =>
    createHmac('sha256', env.JWT_REFRESH_SECRET).update(token).digest('hex');
  const lockUser = (client: PoolClient, userId: string) =>
    client.query('SELECT pg_advisory_xact_lock(hashtextextended($1,0))', [`user:${userId}`]);
  async function transaction<T>(work: (client: PoolClient) => Promise<T>) {
    const client = await db.connect();
    try {
      await client.query('BEGIN');
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
  async function protect(request: FastifyRequest) {
    try {
      await request.jwtVerify();
    } catch {
      throw unauthorized();
    }
    if (
      !(
        await db.query(
          'SELECT 1 FROM refresh_sessions WHERE id=$1 AND user_id=$2 AND revoked_at IS NULL AND expires_at>now()',
          [request.user.sid, request.user.sub],
        )
      ).rowCount
    )
      throw unauthorized();
  }
  async function optionalUser(request: FastifyRequest) {
    if (!request.headers.authorization) return undefined;
    await protect(request);
    return request.user.sub;
  }
  function tokens(user: UserRow, sid: string, refreshToken: string) {
    return {
      user: publicUser(user),
      refreshToken,
      accessToken: app.jwt.sign({ sub: user.id, sid }, { expiresIn: '10m' }),
    };
  }
  async function createSession(client: PoolClient, user: UserRow) {
    const id = randomUUID(),
      token = randomBytes(48).toString('hex');
    await client.query(
      "INSERT INTO refresh_sessions(id,user_id,family_id,token_hash,expires_at) VALUES($1,$2,$1,$3,now()+interval '30 days')",
      [id, user.id, hash(token)],
    );
    return tokens(user, id, token);
  }
  const limited = { config: { rateLimit: { max: 10, timeWindow: '1 minute' } } };
  app.post('/api/v1/auth/register', limited, async (request, reply) => {
    const body = registrationSchema.parse(request.body),
      passwordHash = await hashPassword(body.password);
    try {
      const result = await transaction(async (client) => {
        const user = (
          await client.query<UserRow>(
            `INSERT INTO users(id,email,password_hash,first_name,last_name,date_of_birth,product_updates_consent,product_updates_consent_at,product_updates_consent_version)
          VALUES($1,$2,$3,$4,$5,$6,$7,CASE WHEN $7 THEN now() END,CASE WHEN $7 THEN '2026-09-v1' END) RETURNING ${columns}`,
            [
              randomUUID(),
              body.email,
              passwordHash,
              body.firstName,
              body.lastName || null,
              body.dateOfBirth,
              body.productUpdatesConsent,
            ],
          )
        ).rows[0]!;
        return createSession(client, user);
      });
      return reply.code(201).send(result);
    } catch (error) {
      if ((error as { code?: string }).code === '23505')
        throw new ApiError(
          409,
          'EMAIL_EXISTS',
          'No se puede registrar ese correo. Prueba a iniciar sesión.',
        );
      throw error;
    }
  });
  const dummyHash = hashPassword(randomBytes(32).toString('hex'));
  app.post('/api/v1/auth/login', limited, async (request) => {
    const body = loginSchema.parse(request.body);
    const found = (
      await db.query<{ id: string }>('SELECT id FROM users WHERE email=$1', [body.email])
    ).rows[0];
    if (!found) {
      await argon2.verify(await dummyHash, body.password);
      throw new ApiError(401, 'INVALID_CREDENTIALS', 'El correo o la contraseña no son correctos.');
    }
    return transaction(async (client) => {
      await lockUser(client, found.id);
      const user = (
        await client.query<UserRow>(`SELECT ${columns} FROM users WHERE id=$1`, [found.id])
      ).rows[0];
      const valid = await argon2.verify(user?.password_hash ?? (await dummyHash), body.password);
      if (!valid || !user || user.email !== body.email)
        throw new ApiError(
          401,
          'INVALID_CREDENTIALS',
          'El correo o la contraseña no son correctos.',
        );
      return createSession(client, user);
    });
  });
  app.post('/api/v1/auth/refresh', limited, async (request) => {
    const body = refreshSchema.parse(request.body);
    // Return a sentinel for revoked families so their revocation is COMMITTED before the 401.
    const result = await transaction(async (client) => {
      const family = (
        await client.query<{ family_id: string; user_id: string }>(
          'SELECT family_id,user_id FROM refresh_sessions WHERE token_hash=$1',
          [hash(body.refreshToken)],
        )
      ).rows[0];
      if (!family) return null;
      await lockUser(client, family.user_id);
      await client.query('SELECT pg_advisory_xact_lock(hashtextextended($1,0))', [
        family.family_id,
      ]);
      const session = (
        await client.query<SessionRow>(
          'SELECT * FROM refresh_sessions WHERE token_hash=$1 FOR UPDATE',
          [hash(body.refreshToken)],
        )
      ).rows[0];
      if (!session) return null;
      if (session.revoked_at || session.expires_at.getTime() <= Date.now()) {
        await client.query(
          'UPDATE refresh_sessions SET revoked_at=COALESCE(revoked_at,now()) WHERE family_id=$1',
          [session.family_id],
        );
        return null;
      }
      const user = (
        await client.query<UserRow>(`SELECT ${columns} FROM users WHERE id=$1`, [session.user_id])
      ).rows[0];
      if (!user) return null;
      const id = randomUUID(),
        token = randomBytes(48).toString('hex');
      await client.query('UPDATE refresh_sessions SET revoked_at=now() WHERE id=$1', [session.id]);
      await client.query(
        'INSERT INTO refresh_sessions(id,user_id,family_id,token_hash,expires_at) VALUES($1,$2,$3,$4,$5)',
        [id, user.id, session.family_id, hash(token), session.expires_at],
      );
      return tokens(user, id, token);
    });
    if (!result) throw unauthorized();
    return result;
  });
  app.post('/api/v1/auth/logout', limited, async (request, reply) => {
    const body = refreshSchema.parse(request.body);
    await transaction(async (client) => {
      const family = (
        await client.query<{ family_id: string; user_id: string }>(
          'SELECT family_id,user_id FROM refresh_sessions WHERE token_hash=$1',
          [hash(body.refreshToken)],
        )
      ).rows[0];
      if (!family) return;
      await lockUser(client, family.user_id);
      await client.query('SELECT pg_advisory_xact_lock(hashtextextended($1,0))', [
        family.family_id,
      ]);
      await client.query(
        'UPDATE refresh_sessions SET revoked_at=COALESCE(revoked_at,now()) WHERE family_id=$1',
        [family.family_id],
      );
    });
    return reply.code(204).send();
  });
  app.get('/api/v1/auth/me', { preHandler: protect }, async (request) => {
    const user = (
      await db.query<UserRow>(`SELECT ${columns} FROM users WHERE id=$1`, [request.user.sub])
    ).rows[0];
    if (!user) throw unauthorized();
    return publicUser(user);
  });
  async function currentUser(client: PoolClient, request: FastifyRequest) {
    await lockUser(client, request.user.sub);
    if (
      !(
        await client.query(
          'SELECT 1 FROM refresh_sessions WHERE id=$1 AND user_id=$2 AND revoked_at IS NULL AND expires_at>now()',
          [request.user.sid, request.user.sub],
        )
      ).rowCount
    )
      throw unauthorized();
    const user = (
      await client.query<UserRow>(`SELECT ${columns} FROM users WHERE id=$1 FOR UPDATE`, [
        request.user.sub,
      ])
    ).rows[0];
    if (!user) throw unauthorized();
    return user;
  }
  async function reauthenticate(client: PoolClient, request: FastifyRequest, password: string) {
    const user = await currentUser(client, request);
    if (!(await argon2.verify(user.password_hash, password)))
      throw new ApiError(403, 'REAUTHENTICATION_FAILED', 'La contraseña actual no es correcta.');
    return user;
  }
  for (const kind of ['email', 'password'] as const)
    app.post(`/api/v1/auth/${kind}`, { ...limited, preHandler: protect }, async (request) => {
      const body =
        kind === 'email'
          ? emailChangeSchema.parse(request.body)
          : passwordChangeSchema.parse(request.body);
      try {
        return await transaction(async (client) => {
          const user = await reauthenticate(client, request, body.currentPassword);
          const changed =
            'email' in body
              ? await client.query<UserRow>(
                  `UPDATE users SET email=$2,updated_at=now() WHERE id=$1 RETURNING ${columns}`,
                  [user.id, body.email],
                )
              : await client.query<UserRow>(
                  `UPDATE users SET password_hash=$2,updated_at=now() WHERE id=$1 RETURNING ${columns}`,
                  [user.id, await hashPassword(body.newPassword)],
                );
          await client.query(
            'UPDATE refresh_sessions SET revoked_at=COALESCE(revoked_at,now()) WHERE user_id=$1',
            [user.id],
          );
          return createSession(client, changed.rows[0]!);
        });
      } catch (error) {
        if ((error as { code?: string }).code === '23505')
          throw new ApiError(409, 'EMAIL_EXISTS', 'No se puede utilizar ese correo.');
        throw error;
      }
    });
  app.post(
    '/api/v1/auth/profile/complete',
    { ...limited, preHandler: protect },
    async (request) => {
      const body = profileFieldsSchema.parse(request.body);
      return transaction(async (client) => {
        const user = await currentUser(client, request);
        if (user.first_name !== null || user.date_of_birth !== null)
          throw new ApiError(
            409,
            'PROFILE_LOCKED',
            'Los datos personales ya están completos y no se pueden modificar.',
          );
        const updated = (
          await client.query<UserRow>(
            `UPDATE users SET first_name=$2,last_name=$3,date_of_birth=$4,updated_at=now() WHERE id=$1 RETURNING ${columns}`,
            [user.id, body.firstName, body.lastName || null, body.dateOfBirth],
          )
        ).rows[0]!;
        return publicUser(updated);
      });
    },
  );
  app.delete('/api/v1/auth/me', { ...limited, preHandler: protect }, async (request, reply) => {
    const body = deleteAccountSchema.parse(request.body);
    await transaction(async (client) => {
      await reauthenticate(client, request, body.currentPassword);
      await client.query('DELETE FROM users WHERE id=$1', [request.user.sub]);
    });
    return reply.code(204).send();
  });
  return { protect, optionalUser };
}
export type Auth = ReturnType<typeof authService>;
