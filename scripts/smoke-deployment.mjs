// Runs against an explicitly supplied deployment; creates and removes its own disposable account.
import assert from 'node:assert/strict';
import { randomBytes, randomUUID } from 'node:crypto';
import { mkdir, writeFile } from 'node:fs/promises';
const origin = process.argv[2];
if (!origin || new URL(origin).protocol !== 'https:')
  throw new Error('Supply the HTTPS deployment origin');
const checks = [];
const record = (name, status = 'PASS', detail) => {
  checks.push({ name, status, ...(detail ? { detail } : {}) });
  console.log(`${status}: ${name}${detail ? ` — ${detail}` : ''}`);
};
async function request(path, { method = 'GET', token, body, expected = 200 } = {}) {
  const r = await fetch(`${origin}${path}`, {
    method,
    signal: AbortSignal.timeout(20000),
    headers: {
      'content-type': 'application/json',
      ...(token ? { authorization: `Bearer ${token}` } : {}),
    },
    ...(body ? { body: JSON.stringify(body) } : {}),
  });
  assert.equal(r.status, expected, `${method} ${path}: unexpected HTTP status`);
  return r.status === 204 ? undefined : r.json();
}
const credentials = {
  email: `validation-${randomUUID()}@example.org`,
  password: randomBytes(24).toString('hex'),
};
let session, productShare;
try {
  for (const path of ['/health', '/api-health', '/ready']) {
    await request(path);
    record(path);
  }
  session = await request('/api/v1/auth/register', {
    method: 'POST',
    body: credentials,
    expected: 201,
  });
  record('Register');
  await request('/api/v1/auth/logout', {
    method: 'POST',
    body: { refreshToken: session.refreshToken },
    expected: 204,
  });
  await request('/api/v1/auth/me', { token: session.accessToken, expected: 401 });
  session = await request('/api/v1/auth/login', { method: 'POST', body: credentials });
  session = await request('/api/v1/auth/refresh', {
    method: 'POST',
    body: { refreshToken: session.refreshToken },
  });
  assert.equal(
    (await request('/api/v1/auth/me', { token: session.accessToken })).email,
    credentials.email,
  );
  record('Login, logout, revoked access and refresh');
  const label = await request('/api/v1/products/analyze-text', {
    method: 'POST',
    token: session.accessToken,
    expected: 201,
    body: {
      name: 'Disposable validation label',
      text: 'Sin gluten según receta, puede contener trigo',
      reviewed: true,
    },
  });
  assert.equal(label.assessment.status, 'MAY_CONTAIN_TRACES');
  await request(`/api/v1/products/${label.id}`, { expected: 404 });
  const favorite = `/api/v1/favorites/products/${label.id}`;
  await request(favorite, { method: 'POST', token: session.accessToken, expected: 204 });
  assert.equal((await request(favorite, { token: session.accessToken })).saved, true);
  assert.equal(
    (await request('/api/v1/favorites/products', { token: session.accessToken })).items.length,
    1,
  );
  await request(favorite, { method: 'DELETE', token: session.accessToken, expected: 204 });
  assert.equal((await request(favorite, { token: session.accessToken })).saved, false);
  record('Reviewed text analysis, private label and favorites');
  const share = await request(`/api/v1/shares/products/${label.id}`, {
    method: 'POST',
    token: session.accessToken,
    expected: 201,
  });
  assert.match(share.token, /^[a-f0-9]{48}$/);
  assert.equal(new URL(share.url).origin, origin);
  const publicLabel = await request(`/api/v1/public/shares/${share.token}`);
  assert.ok(!JSON.stringify(publicLabel).includes(credentials.email));
  assert.ok(!JSON.stringify(publicLabel).includes(session.user.id));
  const page = await fetch(share.url);
  assert.equal(page.status, 200);
  const html = await page.text();
  assert.ok(html.includes('property="og:title"'));
  assert.ok(html.includes(`safeeat://p/${share.token}`));
  record('Anonymous share viewer, Open Graph and custom-scheme fallback');
  for (const kind of ['p', 'r']) {
    const missing = await fetch(`${origin}/${kind}/${'f'.repeat(48)}`);
    assert.equal(missing.status, 404);
  }
  record('Product and restaurant viewer 404');
  for (const file of ['apple-app-site-association', 'assetlinks.json']) {
    const r = await fetch(`${origin}/.well-known/${file}`, { redirect: 'manual' });
    assert.equal(r.status, 200);
    await r.json();
  }
  record('Association endpoints (configuration checked separately)');
  const restaurant = await request('/api/v1/restaurants/search?q=Madrid', { expected: 503 });
  assert.equal(restaurant.error.code, 'RESTAURANT_PROVIDER_NOT_CONFIGURED');
  record('Unconfigured restaurant provider returns a clean 503');
  try {
    const p = await request('/api/v1/products/barcode/3017620422003');
    assert.ok(p.assessment.evidence && p.assessment.reasons && p.quality);
    productShare = await request(`/api/v1/shares/products/${p.id}`, {
      method: 'POST',
      expected: 201,
    });
    record(
      'Live Open Food Facts barcode, evidence, nutrition and public share',
      'PASS',
      productShare.url,
    );
    const search = await request('/api/v1/products/search?q=arroz');
    assert.ok(Array.isArray(search.items));
    record('Live Open Food Facts search');
  } catch (error) {
    record('Live Open Food Facts', 'BLOCKED EXTERNALLY', error.message);
  }
  await request('/api/v1/auth/me', { method: 'DELETE', token: session.accessToken, expected: 204 });
  session = undefined;
  await request(`/api/v1/public/shares/${share.token}`, { expected: 404 });
  record('Account deletion revokes private shares');
} catch (error) {
  record('Deployment smoke', 'FAIL', error.message);
  process.exitCode = 1;
} finally {
  if (session)
    await request('/api/v1/auth/me', {
      method: 'DELETE',
      token: session.accessToken,
      expected: 204,
    });
  await mkdir('.local', { recursive: true });
  await writeFile(
    '.local/deployment-validation.json',
    JSON.stringify({ at: new Date().toISOString(), origin, checks, productShare }, null, 2),
  );
}
