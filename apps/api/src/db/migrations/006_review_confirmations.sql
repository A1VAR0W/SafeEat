ALTER TABLE restaurant_catalog ADD COLUMN chain_key text;
ALTER TABLE restaurant_catalog ADD COLUMN chain_name text;
CREATE INDEX restaurant_catalog_chain_idx ON restaurant_catalog(chain_key) WHERE chain_key IS NOT NULL;
ALTER TABLE restaurant_gluten_reports ADD COLUMN chain_key text;
CREATE INDEX restaurant_reports_chain_idx ON restaurant_gluten_reports(chain_key) WHERE chain_key IS NOT NULL;
ALTER TABLE restaurant_gluten_reports ADD COLUMN id uuid NOT NULL DEFAULT gen_random_uuid() UNIQUE;
ALTER TABLE restaurant_gluten_reports ADD COLUMN revision uuid NOT NULL DEFAULT gen_random_uuid();
ALTER TABLE product_gluten_reports ADD COLUMN id uuid NOT NULL DEFAULT gen_random_uuid() UNIQUE;
ALTER TABLE product_gluten_reports ADD COLUMN revision uuid NOT NULL DEFAULT gen_random_uuid();

CREATE TABLE review_confirmations (
 kind text NOT NULL CHECK(kind IN ('products','restaurants')),
 report_id uuid NOT NULL,
 revision uuid NOT NULL,
 user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
 confirmed_at timestamptz NOT NULL DEFAULT now(),
 PRIMARY KEY(kind,report_id,user_id)
);
CREATE FUNCTION invalidate_review_confirmations() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 DELETE FROM review_confirmations WHERE report_id=OLD.id;
 IF TG_OP='DELETE' THEN RETURN OLD; END IF;
 NEW.revision=gen_random_uuid();
 RETURN NEW;
END $$;
CREATE TRIGGER restaurant_review_changed BEFORE UPDATE OR DELETE ON restaurant_gluten_reports
 FOR EACH ROW EXECUTE FUNCTION invalidate_review_confirmations();
CREATE TRIGGER product_review_changed BEFORE UPDATE OR DELETE ON product_gluten_reports
 FOR EACH ROW EXECUTE FUNCTION invalidate_review_confirmations();
