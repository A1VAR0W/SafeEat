import { randomUUID } from 'node:crypto';
import sharp from 'sharp';
import { z } from 'zod';
import type { FastifyInstance } from 'fastify';
import { analyzeSchema, ProductSchema, type Product } from '@safeeat/contracts';
import { assessGluten } from '@safeeat/gluten-engine';
import { scoreNutrition } from '@safeeat/nutrition-engine';
import type { Database } from '../../db/pool';
import type { ProductProvider, ExternalProduct } from '../../providers/types';
import type { Auth } from '../auth';
import { notFound, ApiError } from '../../plugins/errors';
import { reviewRoutes } from '../reviews';
import { reviewNote } from '../permissions';
export function productService(
  app: FastifyInstance,
  db: Database,
  provider: ProductProvider,
  auth: Auth,
  apiPublicUrl: string,
) {
  const pending = new Map<string, Promise<Product>>();
  type Contribution = { id: string; product_name: string; ingredients: string };
  async function latest(code: string) {
    const result = await db.query<Contribution>(
      'SELECT id,product_name,ingredients FROM product_contributions WHERE barcode=$1 ORDER BY created_at DESC LIMIT 1',
      [code],
    );
    return result.rows[0];
  }
  /**
   * Analysis of a label contributed by a user. It can warn (gluten, traces) but never
   * says "gluten free": one person's reading of a label is not a manufacturer's declaration.
   */
  function communityAssessment(ingredients: string): Product['assessment'] {
    const assessment = assessGluten({
      ingredients,
      source: 'Etiqueta revisada por un usuario',
      origin: 'ocr',
      reviewed: true,
    });
    if (assessment.status !== 'GLUTEN_FREE') return assessment;
    return {
      ...assessment,
      status: 'UNKNOWN',
      confidence: 0,
      reasons: [
        {
          code: 'INSUFFICIENT_EVIDENCE',
          message: 'No disponemos de información suficiente para confirmar que sea seguro.',
        },
        {
          code: 'COMMUNITY_CLAIM_UNVERIFIED',
          message:
            'Un usuario ha aportado una etiqueta que dice «sin gluten», pero no está confirmado por el fabricante. Compruébalo en el envase.',
        },
      ],
    };
  }
  /**
   * Adds a user's label without replacing what the provider said. It changes the
   * assessment only to warn of gluten or traces; it never makes a product look safer.
   */
  function contributed(item: Product, contribution?: Contribution): Product {
    if (!contribution) return item;
    const community = communityAssessment(contribution.ingredients);
    const risk = (status: Product['assessment']['status']) =>
      status === 'CONTAINS_GLUTEN'
        ? 3
        : status === 'MAY_CONTAIN_TRACES'
          ? 2
          : status === 'GLUTEN_FREE'
            ? 1
            : 0;
    const warns =
      (community.status === 'CONTAINS_GLUTEN' || community.status === 'MAY_CONTAIN_TRACES') &&
      risk(community.status) > risk(item.assessment.status);
    const fillsIngredients = !item.ingredients;
    return ProductSchema.parse({
      ...item,
      name: item.name.startsWith('Producto ') ? contribution.product_name : item.name,
      ingredients: fillsIngredients ? contribution.ingredients : item.ingredients,
      assessment:
        warns || (fillsIngredients && item.assessment.status === 'UNKNOWN')
          ? community
          : item.assessment,
      ...(warns ? { providerAssessment: item.assessment } : {}),
      informationSource: warns || fillsIngredients ? 'community' : item.informationSource,
      communityContribution: true,
      communityIngredients: contribution.ingredients,
      ingredientsImageUrl: `${apiPublicUrl.replace(/\/$/, '')}/api/v1/products/${encodeURIComponent(item.barcode!)}/ingredients-photo?v=${contribution.id}`,
    });
  }
  async function withReports(item: Product): Promise<Product> {
    if (item.userCreated) return item;
    const reports = await db.query<{
      gluten_free: number;
      contains_gluten: number;
      verified_free: number;
      verified_contains: number;
    }>(
      `WITH reports AS (SELECT r.status,
        EXISTS(SELECT 1 FROM user_roles ur WHERE ur.user_id=r.user_id AND ur.role_code='authorized_celiac')
        OR EXISTS(SELECT 1 FROM review_confirmations c WHERE c.kind='products' AND c.report_id=r.id AND c.revision=r.revision) AS verified
        FROM product_gluten_reports r WHERE r.product_id=$1 AND r.revoked_at IS NULL)
       SELECT count(*) FILTER (WHERE status='GLUTEN_FREE')::int AS gluten_free,
       count(*) FILTER (WHERE status='CONTAINS_GLUTEN')::int AS contains_gluten,
       count(*) FILTER (WHERE status='GLUTEN_FREE' AND verified)::int AS verified_free,
       count(*) FILTER (WHERE status='CONTAINS_GLUTEN' AND verified)::int AS verified_contains FROM reports`,
      [item.id],
    );
    return ProductSchema.parse({
      ...item,
      glutenReports: {
        glutenFree: reports.rows[0]?.gluten_free ?? 0,
        containsGluten: reports.rows[0]?.contains_gluten ?? 0,
      },
      verifiedGlutenReports: {
        glutenFree: reports.rows[0]?.verified_free ?? 0,
        containsGluten: reports.rows[0]?.verified_contains ?? 0,
      },
    });
  }
  function communityProduct(code: string, c: Contribution): Product {
    return ProductSchema.parse({
      id: `off-${code}`,
      barcode: code,
      name: c.product_name,
      ingredients: c.ingredients,
      communityIngredients: c.ingredients,
      assessment: communityAssessment(c.ingredients),
      nutrition: {},
      quality: scoreNutrition({}),
      source: 'Comunidad SafeEat',
      fetchedAt: new Date().toISOString(),
      stale: false,
      userCreated: false,
      informationSource: 'community',
      communityContribution: true,
      ingredientsImageUrl: `${apiPublicUrl.replace(/\/$/, '')}/api/v1/products/${code}/ingredients-photo?v=${c.id}`,
    });
  }
  async function hydrateStored(data: Product, expired: boolean): Promise<Product> {
    const item = ProductSchema.parse({ ...data, stale: expired });
    if (!item.barcode) return item;
    const c = await latest(item.barcode);
    return withReports(
      c && item.source === 'Comunidad SafeEat'
        ? communityProduct(item.barcode, c)
        : contributed(item, c),
    );
  }
  async function save(external: ExternalProduct): Promise<Product> {
    const item = ProductSchema.parse({
      id: `off-${external.barcode}`,
      ...external,
      assessment: assessGluten(external.gluten),
      quality: scoreNutrition(external.nutrition),
      fetchedAt: new Date().toISOString(),
      stale: !!external.summaryOnly,
      userCreated: false,
    });
    if (external.summaryOnly) {
      await db.query(
        'INSERT INTO products(id,barcode,provider,data,expires_at) VALUES($1,$2,$3,$4,now()) ON CONFLICT(id) DO NOTHING',
        [item.id, item.barcode, 'openfoodfacts', JSON.stringify(item)],
      );
      const stored = (
        await db.query<{ data: Product; expired: boolean }>(
          'SELECT data,expires_at<now() AS expired FROM products WHERE id=$1',
          [item.id],
        )
      ).rows[0]!;
      return { ...ProductSchema.parse(stored.data), stale: stored.expired };
    }
    await db.query(
      "INSERT INTO products(id,barcode,provider,data,expires_at) VALUES($1,$2,$3,$4,now()+interval '24 hours') ON CONFLICT(id) DO UPDATE SET provider=$3,data=$4,fetched_at=now(),expires_at=now()+interval '24 hours'",
      [item.id, item.barcode, 'openfoodfacts', JSON.stringify(item)],
    );
    return item;
  }
  async function barcode(code: string): Promise<Product> {
    const cached = await db.query<{ data: Product; expires_at: Date; provider: string }>(
      'SELECT data,expires_at,provider FROM products WHERE barcode=$1 AND owner_id IS NULL',
      [code],
    );
    const row = cached.rows[0];
    const contribution = await latest(code);
    if (
      row &&
      row.expires_at.getTime() > Date.now() &&
      (row.provider !== 'community' || contribution)
    )
      return row.provider === 'community'
        ? communityProduct(code, contribution!)
        : contributed(ProductSchema.parse(row.data), contribution);
    const existing = pending.get(code);
    if (existing) return existing;
    const request = (async () => {
      try {
        const external = await provider.getByBarcode(code);
        if (!external) {
          const c = await latest(code);
          if (c) return communityProduct(code, c);
          throw new ApiError(
            404,
            'PRODUCT_NOT_FOUND',
            'No hemos encontrado este producto. Puedes fotografiar sus ingredientes.',
          );
        }
        return contributed(await save(external), await latest(code));
      } catch (error) {
        if (
          row &&
          (row.provider !== 'community' || contribution) &&
          error instanceof ApiError &&
          error.statusCode === 503
        )
          return contributed({ ...ProductSchema.parse(row.data), stale: true }, contribution);
        if (error instanceof ApiError && error.statusCode === 503) {
          const c = await latest(code);
          if (c) return communityProduct(code, c);
        }
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
    return withReports(row.barcode ? await barcode(row.barcode) : ProductSchema.parse(row.data));
  }
  app.get('/api/v1/products/barcode/:barcode', async (request) => {
    const params = z
      .object({ barcode: z.string().regex(/^(?:\d{8}|\d{12,14})$/) })
      .parse(request.params);
    return withReports(await barcode(params.barcode));
  });
  app.get(
    '/api/v1/products/search',
    { config: { rateLimit: { max: 15, timeWindow: '1 minute' } } },
    async (request) => {
      const { q } = z.object({ q: z.string().trim().min(2).max(100) }).parse(request.query);
      const items = await provider.search(q);
      const external = await Promise.all(
        items.map(async (item) => {
          const stored = await save(item);
          return hydrateStored(stored, stored.stale);
        }),
      );
      const community = await db.query<{
        barcode: string;
        product_name: string;
        ingredients: string;
        id: string;
      }>(
        'SELECT DISTINCT ON (barcode) barcode,product_name,ingredients,id FROM product_contributions WHERE product_name ILIKE $1 ORDER BY barcode,created_at DESC LIMIT 20',
        [`%${q.replace(/[\\%_]/g, '\\$&')}%`],
      );
      const known = new Set(external.map((p) => p.barcode));
      return {
        items: [
          ...external,
          ...(await Promise.all(
            community.rows
              .filter((c) => !known.has(c.barcode))
              .map((c) => withReports(communityProduct(c.barcode, c))),
          )),
        ],
      };
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
        informationSource: 'community',
        communityContribution: false,
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
  app.post(
    '/api/v1/products/:id/gluten-report',
    { preHandler: auth.protect, config: { rateLimit: { max: 8, timeWindow: '1 hour' } } },
    async (request) => {
      const { id } = z.object({ id: z.string().max(80) }).parse(request.params);
      const body = z
        .object({
          status: z.enum(['GLUTEN_FREE', 'CONTAINS_GLUTEN']),
          note: z.string().trim().max(500).default(''),
        })
        .strict()
        .parse(request.body);
      const item = await get(id, request.user.sub);
      body.note = await reviewNote(db, request.user.sub, body.note);
      if (!item.barcode || item.userCreated)
        throw new ApiError(400, 'REPORT_NOT_ALLOWED', 'Esta ficha no admite revisiones públicas.');
      const written = await db.query(
        'INSERT INTO product_gluten_reports(product_id,user_id,status,note) VALUES($1,$2,$3,$4) ON CONFLICT(product_id,user_id) DO UPDATE SET status=$3,note=$4,updated_at=now() WHERE product_gluten_reports.revoked_at IS NULL RETURNING id',
        [item.id, request.user.sub, body.status, body.note],
      );
      if (!written.rowCount)
        throw new ApiError(
          409,
          'REVIEW_REVOKED',
          'Esta revisión ha sido retirada. Contacta con la administración para revisarla.',
        );
      return withReports(item);
    },
  );
  app.post(
    '/api/v1/products/:barcode/ingredients-photo',
    {
      preHandler: auth.protect,
      bodyLimit: 2200000,
      config: { rateLimit: { max: 6, timeWindow: '1 hour' } },
    },
    async (request, reply) => {
      const { barcode: code } = z
        .object({ barcode: z.string().regex(/^(?:\d{8}|\d{12,14})$/) })
        .parse(request.params);
      const body = z
        .object({
          name: z.string().trim().min(1).max(160),
          ingredients: z.string().trim().min(3).max(12000),
          imageBase64: z.string().min(100).max(2000000),
        })
        .strict()
        .parse(request.body);
      const input = Buffer.from(body.imageBase64, 'base64');
      if (
        input.length > 1500000 ||
        input.length < 100 ||
        input[0] !== 0xff ||
        input[1] !== 0xd8 ||
        input[2] !== 0xff ||
        input.toString('base64').replace(/=+$/, '') !== body.imageBase64.replace(/=+$/, '')
      )
        throw new ApiError(400, 'INVALID_IMAGE', 'La foto debe ser JPEG y ocupar menos de 1,5 MB.');
      let photo: Buffer;
      try {
        photo = await sharp(input, { limitInputPixels: 10000000 })
          .rotate()
          .resize(1600, 1600, { fit: 'inside', withoutEnlargement: true })
          .jpeg({ quality: 78 })
          .toBuffer();
      } catch {
        throw new ApiError(
          400,
          'INVALID_IMAGE',
          'No hemos podido leer la foto. Haz una nueva captura.',
        );
      }
      const id = randomUUID();
      await db.query(
        'INSERT INTO product_contributions(id,barcode,product_name,user_id,ingredients,photo,photo_mime) VALUES($1,$2,$3,$4,$5,$6,$7)',
        [id, code, body.name, request.user.sub, body.ingredients, photo, 'image/jpeg'],
      );
      await db.query(
        "INSERT INTO products(id,barcode,provider,data,expires_at) VALUES($1,$2,'community',$3,now()+interval '24 hours') ON CONFLICT(barcode) DO NOTHING",
        [
          `off-${code}`,
          code,
          JSON.stringify(
            communityProduct(code, { id, product_name: body.name, ingredients: body.ingredients }),
          ),
        ],
      );
      return reply.code(201).send(await barcode(code));
    },
  );
  app.get('/api/v1/products/:barcode/ingredients-photo', async (request, reply) => {
    const { barcode: code } = z
      .object({ barcode: z.string().regex(/^(?:\d{8}|\d{12,14})$/) })
      .parse(request.params);
    const result = await db.query<{ photo: Buffer }>(
      'SELECT photo FROM product_contributions WHERE barcode=$1 ORDER BY created_at DESC LIMIT 1',
      [code],
    );
    if (!result.rows[0]) throw notFound();
    return reply
      .header('Content-Type', 'image/jpeg')
      .header('Cache-Control', 'public, max-age=300')
      .send(result.rows[0].photo);
  });
  reviewRoutes(app, db, auth, 'products', async (id, user) => {
    await get(id, user);
    return {};
  });
  return { get, barcode, hydrateStored };
}
export type Products = ReturnType<typeof productService>;
