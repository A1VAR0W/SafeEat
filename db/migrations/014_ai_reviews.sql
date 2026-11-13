-- SafeEat 2.0.3: structured AI review, versioned with the rules.
-- Additive only: the 1.0.x TypeScript API ignores both tables, and the tables of
-- 012/013 stay as they are (product_ai_checks is no longer read: its results were
-- made with free-text answers and without the rule catalog).

-- What the on-server model said about one ingredient (validated against a closed
-- schema). `cache_key` = sha256(normalized ingredient | model | prompt version | rules
-- version): new rules, prompt or model never reuse an old answer.
CREATE TABLE IF NOT EXISTS ingredient_ai_verdicts (
  cache_key text PRIMARY KEY,
  ingredient text NOT NULL,
  verdict text NOT NULL CHECK (verdict IN ('KNOWN_GLUTEN', 'KNOWN_SAFE_INGREDIENT', 'CONTEXT_DEPENDENT', 'INSUFFICIENT_INFORMATION')),
  cereal text,
  model text NOT NULL,
  prompt_version text NOT NULL,
  rules_version text NOT NULL,
  checked_at timestamptz NOT NULL DEFAULT now()
);

-- AI review of one product for one exact analysis. `analysis_key` = sha256(engine
-- version | rules version | model | prompt version | normalized ingredients): a changed
-- ingredient list or a new version is simply reviewed again.
CREATE TABLE IF NOT EXISTS product_ai_reviews (
  product_id text NOT NULL REFERENCES products(id) ON DELETE CASCADE,
  analysis_key text NOT NULL,
  outcome text NOT NULL CHECK (outcome IN ('NO_GLUTEN_SEEN', 'GLUTEN_SUSPECTED', 'UNDETERMINED', 'MODEL_ERROR')),
  -- [{ingredient, verdict, cereal}] as validated, never the model's free text.
  evidence jsonb NOT NULL DEFAULT '[]',
  model text NOT NULL,
  prompt_version text NOT NULL,
  rules_version text NOT NULL,
  engine_version text NOT NULL,
  checked_at timestamptz NOT NULL DEFAULT now(),
  PRIMARY KEY (product_id, analysis_key)
);
