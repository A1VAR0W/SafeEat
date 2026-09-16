import { createHmac, randomBytes, randomUUID } from 'node:crypto';
import argon2 from 'argon2';
import type { FastifyInstance, FastifyRequest } from 'fastify';
import { credentialsSchema, loginSchema, refreshSchema } from '@safeeat/contracts';
import type { Database } from '../../db/pool';
import type { Env } from '../../config/env';
import { ApiError, unauthorized } from '../../plugins/errors';
declare module '@fastify/jwt' {
  interface FastifyJWT {
    payload: { sub: string; sid: string };
    user: { sub: string; sid: string };
  }
}
type UserRow = { id: string; email: string; password_hash: string };
type SessionRow = {
  id: string;
  user_id: string;
  family_id: string;
  expires_at: Date;
  revoked_at: Date | null;
};
export function authService(app: FastifyInstance, db: Database, env: Env) {
  const hash = (token: string) =>
    createHmac('sha256', env.JWT_REFRESH_SECRET).update(token).digest('hex');
  async function protect(request: FastifyRequest) {
    try {
      await request.jwtVerify();
    } catch {
      throw unauthorized();
    }
    const row = await db.query(
      'SELECT 1 FROM refresh_sessions WHERE id=$1 AND user_id=$2 AND revoked_at IS NULL AND expires_at>now()',
      [request.user.sid, request.user.sub],
    );
    if (!row.rowCount) throw unauthorized();
  }
  async function optionalUser(request: FastifyRequest) {
    if (!request.headers.authorization) return undefined;
    await protect(request);
    return request.user.sub;
  }
  function tokens(user: { id: string; email: string }, sid: string, refreshToken: string) {
    return {
      user: { id: user.id, email: user.email },
      refreshToken,
      accessToken: app.jwt.sign({ sub: user.id, sid }, { expiresIn: '10m' }),
    };
  }
  async function createSession(user: UserRow) {
    const id = randomUUID(),
      token = randomBytes(48).toString('hex');
    await db.query(
      "INSERT INTO refresh_sessions(id,user_id,family_id,token_hash,expires_at) VALUES($1,$2,$1,$3,now()+interval '30 days')",
      [id, user.id, hash(token)],
    );
    return tokens(user, id, token);
  }
  const limited = { config: { rateLimit: { max: 10, timeWindow: '1 minute' } } };
  app.post('/api/v1/auth/register', limited, async (request, reply) => {
    const body = credentialsSchema.parse(request.body);
    const passwordHash = await argon2.hash(body.password, {
      type: argon2.argon2id,
      memoryCost: 19456,
      timeCost: 2,
      parallelism: 1,
    });
    let user: UserRow;
    try {
      const result = await db.query<UserRow>(
        'INSERT INTO users(id,email,password_hash) VALUES($1,$2,$3) RETURNING id,email,password_hash',
        [randomUUID(), body.email, passwordHash],
      );
      user = result.rows[0]!;
    } catch (error) {
      if ((error as { code?: string }).code === '23505')
        throw new ApiError(
          409,
          'EMAIL_EXISTS',
          'No se puede registrar ese correo. Prueba a iniciar sesión.',
        );
      throw error;
    }
    return reply.code(201).send(await createSession(user));
  });
  // Keep unknown-account verification comparable to an actual Argon2 check.
  const dummyHash = argon2.hash(randomBytes(32).toString('hex'), {
    type: argon2.argon2id,
    memoryCost: 19456,
    timeCost: 2,
    parallelism: 1,
  });
  app.post('/api/v1/auth/login', limited, async (request) => {
    const body = loginSchema.parse(request.body);
    const result = await db.query<UserRow>(
      'SELECT id,email,password_hash FROM users WHERE email=$1',
      [body.email],
    );
    const user = result.rows[0];
    const valid = await argon2.verify(user?.password_hash ?? (await dummyHash), body.password);
    if (!valid || !user)
      throw new ApiError(401, 'INVALID_CREDENTIALS', 'El correo o la contraseña no son correctos.');
    return createSession(user);
  });
  app.post('/api/v1/auth/refresh', limited, async (request) => {
    const body = refreshSchema.parse(request.body);
    const client = await db.connect();
    try {
      await client.query('BEGIN');
      const rows = await client.query<SessionRow>(
        'SELECT * FROM refresh_sessions WHERE token_hash=$1 FOR UPDATE',
        [hash(body.refreshToken)],
      );
      const session = rows.rows[0];
      if (!session) {
        await client.query('COMMIT');
        throw unauthorized();
      }
      if (session.revoked_at || session.expires_at.getTime() <= Date.now()) {
        await client.query(
          'UPDATE refresh_sessions SET revoked_at=COALESCE(revoked_at,now()) WHERE family_id=$1',
          [session.family_id],
        );
        await client.query('COMMIT');
        throw unauthorized();
      }
      const user = (
        await client.query<UserRow>('SELECT id,email FROM users WHERE id=$1', [session.user_id])
      ).rows[0];
      if (!user) throw unauthorized();
      const id = randomUUID(),
        token = randomBytes(48).toString('hex');
      await client.query('UPDATE refresh_sessions SET revoked_at=now() WHERE id=$1', [session.id]);
      // Absolute family lifetime cannot be prolonged indefinitely by rotation.
      await client.query(
        'INSERT INTO refresh_sessions(id,user_id,family_id,token_hash,expires_at) VALUES($1,$2,$3,$4,$5)',
        [id, user.id, session.family_id, hash(token), session.expires_at],
      );
      await client.query('COMMIT');
      return tokens(user, id, token);
    } catch (error) {
      await client.query('ROLLBACK');
      throw error;
    } finally {
      client.release();
    }
  });
  app.post('/api/v1/auth/logout', limited, async (request, reply) => {
    const body = refreshSchema.parse(request.body);
    await db.query(
      'UPDATE refresh_sessions SET revoked_at=COALESCE(revoked_at,now()) WHERE family_id IN (SELECT family_id FROM refresh_sessions WHERE token_hash=$1)',
      [hash(body.refreshToken)],
    );
    return reply.code(204).send();
  });
  app.get(
    '/api/v1/auth/me',
    { preHandler: protect },
    async (request) =>
      (await db.query('SELECT id,email FROM users WHERE id=$1', [request.user.sub])).rows[0],
  );
  app.delete('/api/v1/auth/me', { preHandler: protect }, async (request, reply) => {
    await db.query('DELETE FROM users WHERE id=$1', [request.user.sub]);
    return reply.code(204).send();
  });
  return { protect, optionalUser };
}
export type Auth = ReturnType<typeof authService>;
