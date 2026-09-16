import { randomUUID } from 'node:crypto';
import { z } from 'zod';
import type { FastifyInstance } from 'fastify';
import { analyzeSchema, ProductSchema, type Product } from '@safeeat/contracts';
import { assessGluten } from '@safeeat/gluten-engine';
import { scoreNutrition } from '@safeeat/nutrition-engine';
import type { Database } from '../../db/pool';
import type { ProductProvider, ExternalProduct } from '../../providers/types';
import type { Auth } from '../auth';
import { notFound, ApiError } from '../../plugins/errors';
export function productService(
  app: FastifyInstance,
  db: Database,
  provider: ProductProvider,
  auth: Auth,
) {
  const pending = new Map<string, Promise<Product>>();
  async function save(external: ExternalProduct): Promise<Product> {
    const item = ProductSchema.parse({
      id: `off-${external.barcode}`,
      ...external,
      assessment: assessGluten(external.gluten),
      quality: scoreNutrition(external.nutrition),
      fetchedAt: new Date().toISOString(),
      stale: false,
      userCreated: false,
    });
    await db.query(
      "INSERT INTO products(id,barcode,provider,data,expires_at) VALUES($1,$2,$3,$4,now()+interval '24 hours') ON CONFLICT(id) DO UPDATE SET data=$4,fetched_at=now(),expires_at=now()+interval '24 hours'",
      [item.id, item.barcode, 'openfoodfacts', JSON.stringify(item)],
    );
    return item;
  }
  async function barcode(code: string): Promise<Product> {
    const cached = await db.query<{ data: Product; expires_at: Date }>(
      'SELECT data,expires_at FROM products WHERE barcode=$1 AND owner_id IS NULL',
      [code],
    );
    const row = cached.rows[0];
    if (row && row.expires_at.getTime() > Date.now()) return ProductSchema.parse(row.data);
    const existing = pending.get(code);
    if (existing) return existing;
    const request = (async () => {
      try {
        const external = await provider.getByBarcode(code);
        if (!external)
          throw new ApiError(
            404,
            'PRODUCT_NOT_FOUND',
            'No hemos encontrado este producto. Puedes escanear sus ingredientes.',
          );
        return await save(external);
      } catch (error) {
        if (row && error instanceof ApiError && error.statusCode === 503)
          return { ...ProductSchema.parse(row.data), stale: true };
        throw error;
      } finally {
        pending.delete(code);
      }
    })();
    pending.set(code, request);
    return request;
  }
  async function get(id: string, userId?: string): Promise<Product> {
    const result = await db.query<{
      data: Product;
      barcode: string | null;
      owner_id: string | null;
    }>(
      'SELECT data,barcode,owner_id FROM products WHERE id=$1 AND (owner_id IS NULL OR owner_id=$2)',
      [id, userId ?? null],
    );
    const row = result.rows[0];
    if (!row) throw notFound();
    return row.barcode ? barcode(row.barcode) : ProductSchema.parse(row.data);
  }
  app.get('/api/v1/products/barcode/:barcode', async (request) => {
    const params = z
      .object({ barcode: z.string().regex(/^(?:\d{8}|\d{12,14})$/) })
      .parse(request.params);
    return barcode(params.barcode);
  });
  app.get(
    '/api/v1/products/search',
    { config: { rateLimit: { max: 15, timeWindow: '1 minute' } } },
    async (request) => {
      const { q } = z.object({ q: z.string().trim().min(2).max(100) }).parse(request.query);
      const items = await provider.search(q);
      return { items: await Promise.all(items.map(save)) };
    },
  );
  app.post(
    '/api/v1/products/analyze-text',
    { preHandler: auth.protect },
    async (request, reply) => {
      const body = analyzeSchema.parse(request.body);
      const item: Product = {
        id: randomUUID(),
        name: body.name,
        ingredients: body.text,
        assessment: assessGluten({
          ingredients: body.text,
          source: 'Etiqueta revisada por el usuario',
          origin: 'ocr',
          reviewed: body.reviewed,
        }),
        nutrition: {},
        quality: scoreNutrition({}),
        source: 'Etiqueta personal',
        fetchedAt: new Date().toISOString(),
        stale: false,
        userCreated: true,
      };
      await db.query(
        "INSERT INTO products(id,provider,owner_id,data,expires_at) VALUES($1,$2,$3,$4,now()+interval '30 days')",
        [item.id, 'user', request.user.sub, JSON.stringify(item)],
      );
      return reply.code(201).send(item);
    },
  );
  app.get('/api/v1/products/:id', async (request) => {
    const { id } = z.object({ id: z.string().max(80) }).parse(request.params);
    return get(id, await auth.optionalUser(request));
  });
  return { get, barcode };
}
export type Products = ReturnType<typeof productService>;
