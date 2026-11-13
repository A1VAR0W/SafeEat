// Usage: OLD=http://localhost:4600/api/v1 NEW=http://localhost:4601/api/v1 node scripts/check-rollback.mjs
// Both APIs must point at the same disposable copy of a production backup, already
// migrated with the new release. Never run it against production or preproduction.
// Rollback check: the 1.x Node API (what production runs) on the schema migrated to
// 2.0.5 (010–018), and the Rust 2.0.5 API on the same database, sharing tokens.
// Run against a disposable copy of the production backup only.
const OLD = process.env.OLD ?? 'http://localhost:4600/api/v1';
const NEW = process.env.NEW ?? 'http://localhost:4601/api/v1';
let failures = 0;
const check = (name, ok, extra) => {
  console.log(
    `${ok ? 'PASS' : 'FAIL'} ${name}${ok ? '' : ' ' + JSON.stringify(extra)?.slice(0, 400)}`,
  );
  if (!ok) failures++;
};
async function call(base, method, path, { token, body } = {}) {
  const res = await fetch(base + path, {
    method,
    headers: {
      ...(body ? { 'content-type': 'application/json' } : {}),
      ...(token ? { authorization: `Bearer ${token}` } : {}),
    },
    body: body ? JSON.stringify(body) : undefined,
  });
  const text = await res.text();
  let json;
  try {
    json = text ? JSON.parse(text) : null;
  } catch {
    json = text;
  }
  return { status: res.status, json };
}
const stamp = Date.now();
const email = `rollback${stamp}@example.test`;
const reg = await call(OLD, 'POST', '/auth/register', {
  body: {
    email,
    password: 'contraseña-larga-1',
    firstName: 'Roll',
    lastName: 'Back',
    dateOfBirth: '1990-01-01',
  },
});
check('1.x register on migrated schema', reg.status === 201, reg);
const T = reg.json?.accessToken;
check('1.x me', (await call(OLD, 'GET', '/auth/me', { token: T })).status === 200);
const area = await call(
  OLD,
  'GET',
  '/restaurants/search?q=restaurante&latitude=40.4168&longitude=-3.7038',
  { token: T },
);
check(
  '1.x restaurant search (Madrid)',
  area.status === 200 && (area.json.items?.length ?? 0) > 0,
  area.status,
);
const venue = area.json.items?.[0]?.id;
if (venue) {
  const report = await call(OLD, 'POST', `/restaurants/${venue}/gluten-report`, {
    token: T,
    body: { support: 'GLUTEN_FREE_OPTIONS', note: 'Prueba de vuelta atrás' },
  });
  check('1.x restaurant report write', report.status >= 200 && report.status < 300, report);
  const fav = await call(OLD, 'POST', `/favorites/restaurants/${venue}`, { token: T });
  check('1.x favourite restaurant write', fav.status >= 200 && fav.status < 300, fav);
}
const product = await call(OLD, 'GET', '/products/barcode/8410044005008', { token: T });
check('1.x product by barcode', product.status === 200 || product.status === 404, product.status);
if (product.status === 200) {
  const fav = await call(OLD, 'POST', `/favorites/products/${product.json.id}`, { token: T });
  check('1.x favourite product write', fav.status >= 200 && fav.status < 300, fav);
}
// Forward again: the 2.0.5 API reads what 1.x wrote, with 1.x's tokens.
const login = await call(NEW, 'POST', '/auth/login', {
  body: { email, password: 'contraseña-larga-1' },
});
check('2.0.5 login with account made by 1.x', login.status === 200, login);
const refresh = await call(NEW, 'POST', '/auth/refresh', {
  body: { refreshToken: reg.json?.refreshToken },
});
check(
  '2.0.5 accepts a 1.x refresh token',
  refresh.status === 200 && refresh.json.accessToken,
  refresh,
);
const favs = await call(NEW, 'GET', '/favorites/products', { token: login.json?.accessToken });
check(
  '2.0.5 sees favourites written by 1.x',
  favs.status === 200 && (favs.json.items?.length ?? 0) >= 1,
  favs,
);
const del = await call(NEW, 'DELETE', '/auth/me', {
  token: login.json?.accessToken,
  body: { currentPassword: 'contraseña-larga-1', confirmation: 'ELIMINAR' },
});
check('test account removed', del.status === 204, del);
console.log(failures ? `\n${failures} FAILED` : '\nALL PASSED');
process.exit(failures ? 1 : 0);
