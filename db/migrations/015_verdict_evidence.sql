-- Personal verdicts remember the food-safety evidence they were given on (allergens,
-- traces, declarations, provenance), not only the ingredient list. Additive: verdicts
-- saved before have NULL and keep the ingredient check only.
ALTER TABLE user_product_verdicts ADD COLUMN IF NOT EXISTS evidence_hash text;
