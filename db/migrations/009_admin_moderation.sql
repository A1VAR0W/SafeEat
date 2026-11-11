INSERT INTO roles(code,name) VALUES('ADMIN','ADMIN');
ALTER TABLE product_gluten_reports DROP CONSTRAINT product_gluten_reports_note_check;
ALTER TABLE restaurant_gluten_reports DROP CONSTRAINT restaurant_gluten_reports_note_check;
ALTER TABLE product_gluten_reports ADD CHECK(length(note)<=500);
ALTER TABLE restaurant_gluten_reports ADD CHECK(length(note)<=500);
ALTER TABLE product_gluten_reports ADD COLUMN revoked_at timestamptz;
ALTER TABLE product_gluten_reports ADD COLUMN revoked_by uuid REFERENCES users(id) ON DELETE SET NULL;
ALTER TABLE product_gluten_reports ADD COLUMN revocation_reason text;
ALTER TABLE restaurant_gluten_reports ADD COLUMN revoked_at timestamptz;
ALTER TABLE restaurant_gluten_reports ADD COLUMN revoked_by uuid REFERENCES users(id) ON DELETE SET NULL;
ALTER TABLE restaurant_gluten_reports ADD COLUMN revocation_reason text;
CREATE TABLE admin_audit_log (
 id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
 actor_id uuid REFERENCES users(id) ON DELETE SET NULL,
 action text NOT NULL,
 target_id text NOT NULL,
 details jsonb NOT NULL DEFAULT '{}',
 created_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX admin_audit_log_created_idx ON admin_audit_log(created_at DESC);
