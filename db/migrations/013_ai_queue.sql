-- SafeEat 2.0: AI checks waiting to run survive a server restart.
-- Additive only: the 1.0.x TypeScript API ignores this table.
CREATE TABLE IF NOT EXISTS product_ai_queue (
  product_id text PRIMARY KEY REFERENCES products(id) ON DELETE CASCADE,
  ingredients_hash text NOT NULL,
  items text[] NOT NULL,
  queued_at timestamptz NOT NULL DEFAULT now()
);
