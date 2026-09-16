import pg from 'pg';
export function createPool(connectionString: string) {
  return new pg.Pool({
    connectionString,
    max: 10,
    idleTimeoutMillis: 30000,
    connectionTimeoutMillis: 5000,
    statement_timeout: 10000,
  });
}
export type Database = ReturnType<typeof createPool>;
