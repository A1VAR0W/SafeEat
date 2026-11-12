-- SafeEat 2.0: Spanish ingredient names and the on-server AI second opinion.
-- Additive only: the 1.0.x TypeScript API ignores both tables.

-- Spanish names from the Open Food Facts taxonomy. `name` NULL means OFF has no
-- Spanish name (cached too, so the same id is not asked again until it expires).
CREATE TABLE IF NOT EXISTS taxonomy_names_es (
  tagtype text NOT NULL CHECK (tagtype IN ('ingredients', 'allergens')),
  id text NOT NULL,
  name text,
  fetched_at timestamptz NOT NULL DEFAULT now(),
  PRIMARY KEY (tagtype, id)
);

-- Result of the on-server model for one product and one exact ingredient list.
-- A changed ingredient list (different hash) is simply checked again.
CREATE TABLE IF NOT EXISTS product_ai_checks (
  product_id text NOT NULL REFERENCES products(id) ON DELETE CASCADE,
  ingredients_hash text NOT NULL,
  status text NOT NULL CHECK (status IN ('VERIFIED', 'SUSPECTED')),
  flagged text[] NOT NULL DEFAULT '{}',
  model text NOT NULL,
  checked_at timestamptz NOT NULL DEFAULT now(),
  PRIMARY KEY (product_id, ingredients_hash)
);
