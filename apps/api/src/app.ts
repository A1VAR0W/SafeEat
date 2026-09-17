import Fastify from 'fastify';
import cors from '@fastify/cors';
import helmet from '@fastify/helmet';
import jwt from '@fastify/jwt';
import rateLimit from '@fastify/rate-limit';
import { ZodError } from 'zod';
import type { Env } from './config/env';
import { createPool, type Database } from './db/pool';
import type { ProductProvider, RestaurantProvider } from './providers/types';
import { OpenFoodFactsProvider } from './providers/open-food-facts';
import { GooglePlacesProvider } from './providers/google-places';
import { authService } from './modules/auth';
import { productService } from './modules/products';
import { restaurantService } from './modules/restaurants';
import { favoriteRoutes } from './modules/favorites';
import { sharingRoutes } from './modules/sharing';
import { ApiError } from './plugins/errors';
import { version } from '../../../package.json';
export async function createApp(
  env: Env,
  options: {
    db?: Database;
    products?: ProductProvider;
    restaurants?: RestaurantProvider;
    logging?: boolean;
  } = {},
) {
  const db = options.db ?? createPool(env.DATABASE_URL);
  const app = Fastify({
    bodyLimit: 32768,
    trustProxy: (_address, hop) => hop < env.TRUST_PROXY,
    logger:
      options.logging === false
        ? false
        : {
            level: 'info',
            redact: [
              'req.headers.authorization',
              'req.headers.cookie',
              'res.headers["set-cookie"]',
              'password',
              'refreshToken',
              'accessToken',
            ],
            serializers: {
              req: (request) => ({
                method: request.method,
                url: request.url?.split('?')[0],
                hostname: request.hostname,
              }),
            },
          },
    disableRequestLogging: true,
  });
  await app.register(cors, {
    origin: new URL(env.PUBLIC_APP_URL).origin,
    methods: ['GET', 'POST', 'DELETE'],
    allowedHeaders: ['Content-Type', 'Authorization'],
  });
  await app.register(helmet);
  await app.register(jwt, {
    secret: env.JWT_ACCESS_SECRET,
    sign: { iss: 'safeeat', aud: 'safeeat-mobile' },
    verify: { allowedIss: 'safeeat', allowedAud: 'safeeat-mobile', algorithms: ['HS256'] },
  });
  await app.register(rateLimit, {
    max: 100,
    timeWindow: '1 minute',
    errorResponseBuilder: () => ({
      statusCode: 429,
      error: {
        code: 'RATE_LIMIT',
        message: 'Demasiadas solicitudes. Espera un momento e inténtalo de nuevo.',
      },
    }),
  });
  app.setErrorHandler((error, request, reply) => {
    if (error instanceof ZodError)
      return reply
        .code(400)
        .send({ error: { code: 'INVALID_INPUT', message: 'Revisa los datos introducidos.' } });
    if (error instanceof ApiError)
      return reply
        .code(error.statusCode)
        .send({ error: { code: error.code, message: error.message } });
    const known = error as { statusCode?: number; name?: string };
    if (known.statusCode === 429)
      return reply.code(429).send({
        error: { code: 'RATE_LIMIT', message: 'Demasiadas solicitudes. Espera un momento.' },
      });
    if (known.statusCode && known.statusCode < 500)
      return reply
        .code(known.statusCode)
        .send({ error: { code: 'INVALID_REQUEST', message: 'La solicitud no es válida.' } });
    request.log.error({ requestId: request.id, errorType: known.name }, 'Request failed');
    return reply.code(500).send({
      error: {
        code: 'INTERNAL_ERROR',
        message: 'No hemos podido completar la operación. Inténtalo más tarde.',
      },
    });
  });
  app.setNotFoundHandler((_request, reply) =>
    reply
      .code(404)
      .send({ error: { code: 'NOT_FOUND', message: 'No hemos encontrado este recurso.' } }),
  );
  app.addHook('onSend', async (request, reply, payload) => {
    if (request.url.startsWith('/api/')) reply.header('Cache-Control', 'no-store');
    return payload;
  });
  const metadata = () => ({ status: 'ok', environment: env.APP_ENV, version });
  app.get('/health', async () => metadata());
  app.get('/api/v1/meta', async () => metadata());
  app.get('/ready', async (_request, reply) => {
    try {
      await db.query('SELECT 1');
      return { status: 'ready' };
    } catch {
      return reply.code(503).send({ status: 'unavailable' });
    }
  });
  const auth = authService(app, db, env);
  const products = productService(
    app,
    db,
    options.products ??
      new OpenFoodFactsProvider(env.OPENFOODFACTS_USER_AGENT, env.OPENFOODFACTS_BASE_URL),
    auth,
  );
  const restaurants = restaurantService(
    app,
    options.restaurants ?? new GooglePlacesProvider(env.GOOGLE_PLACES_API_KEY),
  );
  favoriteRoutes(app, db, auth, products, restaurants);
  sharingRoutes(app, db, auth, products, restaurants, env.PUBLIC_APP_URL.replace(/\/$/, ''));
  app.addHook('onClose', async () => {
    if (!options.db) await db.end();
  });
  return app;
}
