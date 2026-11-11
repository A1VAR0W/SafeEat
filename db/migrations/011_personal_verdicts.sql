-- SafeEat 2.0: a personal "safe / not safe for me" verdict per product.
-- It is shown above every other source for its owner but never changes the
-- automatic assessment. ingredients_hash detects later ingredient changes.
CREATE TABLE user_product_verdicts (
 user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
 product_id text NOT NULL REFERENCES products(id) ON DELETE CASCADE,
 verdict text NOT NULL CHECK(verdict IN ('SAFE','UNSAFE')),
 note text NOT NULL DEFAULT '' CHECK(length(note)<=500),
 ingredients_hash text,
 updated_at timestamptz NOT NULL DEFAULT now(),
 PRIMARY KEY(user_id,product_id)
);
CREATE INDEX user_product_verdicts_user_idx ON user_product_verdicts(user_id,updated_at DESC);

-- A product row referenced by a personal verdict is kept even if its community
-- contribution disappears (it would otherwise cascade-delete the verdict).
CREATE OR REPLACE FUNCTION remove_orphan_community_product() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NOT EXISTS (SELECT 1 FROM product_contributions WHERE barcode=OLD.barcode) THEN
   DELETE FROM products p WHERE p.barcode=OLD.barcode AND p.provider='community'
     AND NOT EXISTS (SELECT 1 FROM user_product_verdicts v WHERE v.product_id=p.id)
     AND NOT EXISTS (SELECT 1 FROM product_gluten_reports r WHERE r.product_id=p.id)
     AND NOT EXISTS (SELECT 1 FROM favorite_products f WHERE f.product_id=p.id);
 END IF;
 RETURN NULL;
END $$;

-- Audit trail for AI label analyses (no image is stored).
CREATE TABLE label_analyses (
 id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
 user_id uuid REFERENCES users(id) ON DELETE CASCADE,
 barcode text,
 ai_status text NOT NULL,
 final_status text NOT NULL,
 model text NOT NULL,
 created_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX label_analyses_user_idx ON label_analyses(user_id,created_at DESC);
