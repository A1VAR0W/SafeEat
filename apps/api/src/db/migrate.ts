import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import type { Database } from './pool';
import { createPool } from './pool';
export async function migrate(
  db: Database,
  directory = process.env.MIGRATIONS_DIR ?? resolve('src/db/migrations'),
) {
  const client = await db.connect();
  try {
    await client.query('BEGIN');
    await client.query('SELECT pg_advisory_xact_lock(87349102)');
    await client.query(
      'CREATE TABLE IF NOT EXISTS schema_migrations (name text PRIMARY KEY, applied_at timestamptz NOT NULL DEFAULT now())',
    );
    for (const name of (await readdir(directory)).filter((n) => n.endsWith('.sql')).sort()) {
      const existing = await client.query('SELECT 1 FROM schema_migrations WHERE name=$1', [name]);
      if (existing.rowCount) continue;
      await client.query(await readFile(resolve(directory, name), 'utf8'));
      await client.query('INSERT INTO schema_migrations(name) VALUES($1)', [name]);
    }
    await client.query('COMMIT');
  } catch (error) {
    await client.query('ROLLBACK');
    throw error;
  } finally {
    client.release();
  }
}
if (process.argv[1] && fileURLToPath(import.meta.url) === resolve(process.argv[1])) {
  const db = createPool(process.env.DATABASE_URL ?? '');
  try {
    await migrate(db);
    console.log('Migrations applied');
  } finally {
    await db.end();
  }
}
