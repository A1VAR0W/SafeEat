import { build } from 'esbuild';
await build({
  entryPoints: ['src/server.ts', 'src/db/migrate.ts'],
  outdir: 'dist',
  bundle: true,
  platform: 'node',
  format: 'esm',
  target: 'node22',
  external: ['fastify', '@fastify/*', 'argon2', 'pg'],
  sourcemap: false,
});
