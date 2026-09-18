import assert from 'node:assert/strict';
import { writeFile } from 'node:fs/promises';
const env = process.env.APP_ENV;
assert.ok(['production', 'preproduction'].includes(env), 'Release requires explicit APP_ENV');
for (const key of ['API_PUBLIC_URL', 'PUBLIC_APP_URL']) {
  assert.ok(process.env[key], `Missing ${key}`);
  const u = new URL(process.env[key]);
  assert.ok(
    u.protocol === 'https:' && !u.username && !u.password && !u.search && !u.hash,
    `Invalid ${key}`,
  );
  assert.ok(
    !/^(localhost|.*\.localhost|.*\.local|.*\.lan|\d+\.\d+\.\d+\.\d+|\[.*\])$/i.test(u.hostname) &&
      u.hostname.includes('.'),
    `Non-public ${key}`,
  );
}
const apiUrl = process.env.API_PUBLIC_URL.replace(/\/$/, '');
let serverVersion = null;
let healthChecked = false;
try {
  const response = await fetch(`${apiUrl}/api/v1/meta`, {
    redirect: 'error',
    signal: AbortSignal.timeout(20000),
  });
  assert.equal(response.status, 200, 'Public API metadata not reachable');
  const meta = await response.json();
  assert.equal(meta.environment, env, 'Build target differs from backend environment');
  assert.equal(meta.status, 'ok');
  const web = await fetch(`${process.env.PUBLIC_APP_URL}/health`, {
    redirect: 'error',
    signal: AbortSignal.timeout(20000),
  });
  assert.equal(web.status, 200, 'Public web health not reachable');
  serverVersion = meta.version;
  healthChecked = true;
} catch (error) {
  // Tailscale MagicDNS is not always resolvable on hosted GitHub runners.
  // The release operator must verify both endpoints from the tailnet before publishing.
  if (!(process.env.GITHUB_ACTIONS === 'true' && error?.cause?.code === 'ENOTFOUND')) throw error;
  console.warn(
    'Release URL DNS unavailable on this runner; server health must be checked before publishing.',
  );
}
await writeFile(
  'artifacts/SafeEat-environment.json',
  JSON.stringify(
    {
      environment: env,
      apiUrl,
      publicUrl: process.env.PUBLIC_APP_URL,
      serverVersion,
      healthChecked,
      verifiedAt: new Date().toISOString(),
    },
    null,
    2,
  ),
);
console.log(
  healthChecked
    ? `Verified ${env} backend over HTTPS (server ${serverVersion})`
    : `Validated ${env} release URL configuration`,
);
