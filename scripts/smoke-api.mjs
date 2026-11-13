// End-to-end smoke test of the Rust API against a disposable database (never production).
// Usage: API=http://localhost:4555/api/v1 PSQL="docker exec safeeat-pg-test psql -qtA -U postgres -d safeeat" node scripts/smoke-api.mjs
// Expects the seed in the PostGIS test container: Burger King (24, Q177054) and Rodilla (5,
// brand only) around Madrid, plus five "Mahou"-tagged bars that must not form a chain.
import { execSync } from 'node:child_process';

const BASE = process.env.API ?? 'http://localhost:4555/api/v1';
const PSQL = process.env.PSQL ?? 'docker exec safeeat-pg-test psql -qtA -U postgres -d safeeat';
const sql = (q) => execSync(`${PSQL} -c "${q}"`).toString().trim();
let failures = 0;
const check = (name, ok, extra) => {
  console.log(
    `${ok ? 'PASS' : 'FAIL'} ${name}${ok ? '' : ' ' + JSON.stringify(extra)?.slice(0, 600)}`,
  );
  if (!ok) failures++;
};
// The server's limits stay as they are: on 429 the test waits as an app would.
async function call(method, path, options = {}) {
  let res = await once(method, path, options);
  if (res.status === 429 && (res.json?.error?.retryAfter ?? 99) <= 65) {
    console.log(`   (429 on ${path}: waiting ${res.json.error.retryAfter}s)`);
    await new Promise((ok) => setTimeout(ok, (res.json.error.retryAfter + 1) * 1000));
    res = await once(method, path, options);
  }
  return res;
}
async function once(method, path, { token, body } = {}) {
  const res = await fetch(BASE + path, {
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
const register = (who) =>
  call('POST', '/auth/register', {
    body: {
      email: `${who}${stamp}@example.test`,
      password: 'contraseña-larga-1',
      firstName: 'Test',
      lastName: who,
      dateOfBirth: '1990-01-01',
    },
  });

const trusted = await register('trusted');
check(
  'register',
  trusted.status === 201 && trusted.json.accessToken && trusted.json.refreshToken,
  trusted,
);
const normal = await register('normal');
const admin = await register('admin');
const T = trusted.json.accessToken,
  N = normal.json.accessToken;
sql(
  `INSERT INTO user_roles(user_id,role_code) VALUES('${trusted.json.user.id}','authorized_celiac'),('${admin.json.user.id}','ADMIN')`,
);

const dup = await register('trusted').then(() =>
  call('POST', '/auth/register', {
    body: {
      email: `trusted${stamp}@example.test`,
      password: 'contraseña-larga-1',
      firstName: 'A',
      lastName: 'B',
      dateOfBirth: '1990-01-01',
    },
  }),
);
check('duplicate email 409', dup.status === 409 && dup.json.error.code === 'EMAIL_EXISTS', dup);

const login = await call('POST', '/auth/login', {
  body: { email: `normal${stamp}@example.test`, password: 'contraseña-larga-1' },
});
check('login', login.status === 200 && login.json.accessToken, login);
const bad = await call('POST', '/auth/login', {
  body: { email: `normal${stamp}@example.test`, password: 'mala' },
});
check('bad login 401', bad.status === 401 && bad.json.error.code === 'INVALID_CREDENTIALS', bad);
const me = await call('GET', '/auth/me', { token: T });
check('me', me.status === 200 && me.json.email?.startsWith('trusted'), me);
const refreshed = await call('POST', '/auth/refresh', {
  body: { refreshToken: login.json.refreshToken },
});
check(
  'refresh',
  refreshed.status === 200 && refreshed.json.refreshToken !== login.json.refreshToken,
  refreshed,
);
const reuse = await call('POST', '/auth/refresh', {
  body: { refreshToken: login.json.refreshToken },
});
check('refresh reuse rejected', reuse.status === 401, reuse);
const noAuth = await call('GET', '/auth/me');
check('me without token 401', noAuth.status === 401, noAuth);

// Restaurants & chains
const MAD = 'latitude=40.4168&longitude=-3.7038';
const sug = await call('GET', `/restaurants/suggest?q=burger%20k&includeChains=true&${MAD}`);
check(
  'suggest chain first',
  sug.status === 200 &&
    sug.json.items?.[0]?.chainKey === 'Q177054' &&
    !sug.json.items.some((s) => !s.id),
  sug,
);
// 1.0.12 parses kind with z.enum(['restaurant','place']): without the opt-in no chain item may appear.
const sugOld = await call('GET', `/restaurants/suggest?q=burger%20k&localOnly=true&${MAD}`);
check(
  'suggest stays 1.0.12-compatible',
  sugOld.status === 200 &&
    sugOld.json.items?.length > 0 &&
    sugOld.json.items.every((s) => ['restaurant', 'place'].includes(s.kind)),
  sugOld,
);
console.log('   suggest:', sug.json.items?.map((s) => s.label ?? s.name).join(' | '));
const sugAcc = await call('GET', `/restaurants/suggest?q=meson%20angel&${MAD}`);
check(
  'suggest accent-insensitive',
  sugAcc.json.items?.some((s) => (s.label ?? s.name ?? '').includes('ÁNGEL')),
  sugAcc,
);
const chainSearch = await call('GET', `/restaurants/search?q=Burger%20King&chain=Q177054&${MAD}`);
const ids = chainSearch.json.items?.map((r) => r.id);
check(
  'chain search nearest first',
  chainSearch.status === 200 &&
    ids?.[0] === 'osm_n_1' &&
    ids.includes('osm_n_2') &&
    !ids.includes('osm_n_4'),
  chainSearch,
);
const nameSearch = await call('GET', `/restaurants/search?q=rodilla&${MAD}`);
check('name search', nameSearch.status === 200 && nameSearch.json.items?.length === 5, nameSearch);
const chains = await call('GET', '/restaurants/chains?q=rod');
check(
  'chains list',
  chains.status === 200 && chains.json.items?.[0]?.key === 'brand:rodilla',
  chains,
);
const detailN = await call('GET', `/restaurants/chains/Q177054?${MAD}`, { token: N });
check(
  'chain detail (normal)',
  detailN.status === 200 && detailN.json.canMarkChain === false,
  detailN,
);
const detailT = await call('GET', `/restaurants/chains/Q177054?${MAD}`, { token: T });
check(
  'chain detail (trusted)',
  detailT.status === 200 && detailT.json.canMarkChain === true,
  detailT,
);
console.log('   chain detail keys:', Object.keys(detailT.json ?? {}).join(','));
const badKey = await call('GET', '/restaurants/chains/DROP%20TABLE');
check('chain key validated', badKey.status === 400 || badKey.status === 404, badKey);

const forbidden = await call('POST', '/restaurants/osm_n_2/gluten-report', {
  token: N,
  body: { support: 'GLUTEN_FREE_OPTIONS', note: 'Tienen pan sin gluten', scope: 'chain' },
});
check(
  'chain report normal → 403',
  forbidden.status === 403 && forbidden.json.error.code === 'CHAIN_REVIEW_FORBIDDEN',
  forbidden,
);
const chainReport = await call('POST', '/restaurants/osm_n_2/gluten-report', {
  token: T,
  body: {
    support: 'GLUTEN_FREE_OPTIONS',
    note: 'Pan sin gluten y freidora aparte',
    scope: 'chain',
  },
});
check('chain report trusted', chainReport.status >= 200 && chainReport.status < 300, chainReport);
const localReport = await call('POST', '/restaurants/osm_n_3/gluten-report', {
  token: N,
  body: { support: 'NO_GLUTEN_FREE_OPTIONS', note: 'En este local no tienen nada' },
});
check('local report normal', localReport.status >= 200 && localReport.status < 300, localReport);
const sol = await call('GET', '/restaurants/osm_n_3', { token: N });
const callao = await call('GET', '/restaurants/osm_n_1', { token: N });
check('local overrides chain', sol.json.glutenSupport === 'NO_GLUTEN_FREE_OPTIONS', sol.json);
check('chain applies to sibling', callao.json.glutenSupport === 'GLUTEN_FREE_OPTIONS', callao.json);
const rating = await call('PUT', '/restaurants/osm_n_1/rating', { token: N, body: { stars: 4 } });
check('rating put', rating.status === 200, rating);
const reviews = await call('GET', '/restaurants/osm_n_1/reviews', { token: N });
check('restaurant reviews', reviews.status === 200 && Array.isArray(reviews.json.items), reviews);

// Products
const analyzed = await call('POST', '/products/analyze-text', {
  token: N,
  body: { name: 'Galletas', text: 'Harina de trigo, azúcar', reviewed: true },
});
check(
  'analyze-text gluten',
  analyzed.status === 201 && JSON.stringify(analyzed.json).includes('CONTAINS_GLUTEN'),
  analyzed,
);
// A photo reading she has not checked never reassures, and stays private to her.
const unchecked = await call('POST', '/products/analyze-text', {
  token: N,
  body: { text: 'Arroz, aceite de girasol, sal.' },
});
check(
  'unchecked label text → insufficient information',
  unchecked.status === 201 &&
    unchecked.json.assessment.status === 'UNKNOWN' &&
    unchecked.json.assessment.finding === 'INSUFFICIENT_INFORMATION',
  unchecked.json?.assessment,
);
if (unchecked.status === 201) {
  const ownLabel = await call('GET', `/products/${unchecked.json.id}`, { token: N });
  check('own label readable', ownLabel.status === 200, ownLabel);
  const otherLabel = await call('GET', `/products/${unchecked.json.id}`, { token: T });
  check('label private to its owner (404 for others)', otherLabel.status === 404, otherLabel);
}
const photo = await call('POST', '/products/analyze-photo', {
  token: N,
  body: { imageBase64: '/9j/' + 'A'.repeat(200) },
});
check(
  'no paid AI endpoint (analyze-photo removed)',
  photo.status === 404 || photo.status === 405,
  photo,
);
const product = await call('GET', '/products/barcode/3017620422003', { token: N });
check('barcode lookup (OFF network)', product.status === 200 && product.json.id, product);
if (product.status === 200) {
  const pid = product.json.id;
  console.log(
    '   product:',
    product.json.name,
    product.json.assessment?.status,
    product.json.assessment?.certification ?? '-',
  );
  const v = await call('PUT', `/products/${pid}/verdict`, {
    token: N,
    body: { verdict: 'UNSAFE', note: 'Me sentó mal' },
  });
  check('verdict put', v.status === 200, v);
  const again = await call('GET', `/products/${pid}`, { token: N });
  check(
    'product shows my_verdict',
    again.json.myVerdict?.verdict === 'UNSAFE' && again.json.myVerdict.ingredientsChanged === false,
    again.json.myVerdict,
  );
  const mine = await call('GET', '/products/verdicts', { token: N });
  check('my verdicts list', mine.json.items?.length === 1, mine);
  const other = await call('GET', `/products/${pid}`, { token: T });
  check('verdict private to owner', !other.json.myVerdict, other.json.myVerdict);
  const fav = await call('POST', `/favorites/products/${pid}`, { token: N });
  check('favorite product', fav.status < 300, fav);
  const favs = await call('GET', '/favorites/products', { token: N });
  check('favorites list', favs.json.items?.[0]?.id === pid, favs);
  const col = await call('POST', '/collections', { token: N, body: { name: 'Súper' } });
  check('create collection', col.status < 300, col);
  const share = await call('POST', `/shares/products/${pid}`, { token: N });
  check('share product', share.status < 300 && share.json.token, share);
  if (share.json?.token) {
    const pub = await call('GET', `/public/shares/${share.json.token}`);
    check(
      'public share hides verdict',
      pub.status === 200 && !JSON.stringify(pub.json).includes('myVerdict'),
      pub,
    );
  }
  const del = await call('DELETE', `/products/${pid}/verdict`, { token: N });
  check('verdict delete 204', del.status === 204, del);
}
const search = await call('GET', '/products/search?q=galletas%20sin%20gluten&glutenFree=true', {
  token: N,
});
check(
  'product search (OFF network)',
  search.status === 200 && Array.isArray(search.json.items),
  search,
);
console.log(
  '   search results:',
  search.json.items?.length,
  search.json.items
    ?.slice(0, 3)
    .map((p) => `${p.name}:${p.assessment?.status}`)
    .join(' | '),
);

// Admin
const adminUsers = await call('GET', '/admin/users', { token: admin.json.accessToken });
check('admin users', adminUsers.status === 200, adminUsers);
const notAdmin = await call('GET', '/admin/users', { token: N });
check('admin forbidden for normal', notAdmin.status === 403, notAdmin);
const adminReviews = await call('GET', '/admin/reviews?kind=restaurants', {
  token: admin.json.accessToken,
});
check('admin reviews', adminReviews.status === 200, adminReviews);

// Trusted reviews are verified by the role in the database, never by what the app says.
const A = admin.json.accessToken;
const trustedId = trusted.json.user.id;
const trustedReviewOf = async () => {
  const list = await call('GET', '/restaurants/osm_n_2/reviews', { token: N });
  return list.json.items?.find((r) => r.verificationMethod === 'authorized');
};
const auto = await trustedReviewOf();
check('trusted review verified automatically', !!auto?.verifiedAt, auto);
const normalReviews = await call('GET', '/restaurants/osm_n_3/reviews', { token: T });
const plain = normalReviews.json.items?.find((r) => r.note === 'En este local no tienen nada');
check(
  'normal review not verified',
  !!plain && !plain.verifiedAt && plain.verificationMethod == null,
  plain,
);
const selfGrant = await call('PUT', `/admin/users/${normal.json.user.id}/authorized`, {
  token: N,
});
check('grant by non-admin → 403', selfGrant.status === 403, selfGrant);
const revokeByNormal = await call('DELETE', `/admin/users/${trustedId}/authorized`, { token: N });
check('revoke by non-admin → 403', revokeByNormal.status === 403, revokeByNormal);
const revoked = await call('DELETE', `/admin/users/${trustedId}/authorized`, { token: A });
check('admin revokes trust', revoked.status === 200 && revoked.json.authorized === false, revoked);
const afterRevoke = await call('GET', '/restaurants/osm_n_2/reviews', { token: N });
const sameReview = afterRevoke.json.items?.find((r) => r.id === auto?.id);
check(
  'revoked trust → review no longer verified',
  !!sameReview && !sameReview.verifiedAt && sameReview.verificationMethod == null,
  sameReview,
);
const regrant = await call('PUT', `/admin/users/${trustedId}/authorized`, { token: A });
check('admin grants trust', regrant.status === 200 && regrant.json.authorized === true, regrant);
const audit = await call('GET', '/admin/audit-log', { token: A });
check(
  'audit log lists role changes',
  audit.status === 200 &&
    audit.json.items.some(
      (e) =>
        e.action === 'authorized_removed' &&
        e.targetEmail === `trusted${stamp}@example.test` &&
        e.actorEmail === `admin${stamp}@example.test`,
    ),
  audit.json?.items?.slice(0, 3),
);
check(
  'audit log newest first',
  audit.json.items?.[0]?.action === 'authorized_granted',
  audit.json?.items?.slice(0, 2),
);
const auditNormal = await call('GET', '/admin/audit-log', { token: N });
check('audit log forbidden for normal', auditNormal.status === 403, auditNormal);

// Account deletion: password and the typed word, checked by the server.
const leaving = await register('leaving');
const L = leaving.json.accessToken;
const noWord = await call('DELETE', '/auth/me', {
  token: L,
  body: { currentPassword: 'contraseña-larga-1', confirmation: 'eliminar' },
});
check(
  'delete without the exact word → 400',
  noWord.status === 400 && noWord.json.error.code === 'CONFIRMATION_REQUIRED',
  noWord,
);
const noBody = await call('DELETE', '/auth/me', { token: L });
check('delete without body → 400', noBody.status === 400, noBody);
const wrongPw = await call('DELETE', '/auth/me', {
  token: L,
  body: { currentPassword: 'otra-contraseña', confirmation: 'ELIMINAR' },
});
check(
  'delete with wrong password refused',
  wrongPw.status === 401 || wrongPw.status === 403,
  wrongPw,
);
const stillThere = await call('POST', '/auth/login', {
  body: { email: `leaving${stamp}@example.test`, password: 'contraseña-larga-1' },
});
check('account intact after refused deletion', stillThere.status === 200, stillThere);
const deleted = await call('DELETE', '/auth/me', {
  token: stillThere.json.accessToken,
  body: { currentPassword: 'contraseña-larga-1', confirmation: 'ELIMINAR' },
});
check('delete with password and word → 204', deleted.status === 204, deleted);
const gone = await call('POST', '/auth/login', {
  body: { email: `leaving${stamp}@example.test`, password: 'contraseña-larga-1' },
});
check('deleted account cannot log in', gone.status === 401, gone);

console.log(failures ? `\n${failures} FAILED` : '\nALL PASSED');
process.exit(failures ? 1 : 0);
