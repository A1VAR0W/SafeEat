CREATE TABLE product_contributions (
 id uuid PRIMARY KEY,
 barcode text NOT NULL CHECK(barcode ~ '^[0-9]{8,14}$'),
 product_name text NOT NULL,
 user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
 ingredients text NOT NULL,
 photo bytea NOT NULL,
 photo_mime text NOT NULL CHECK(photo_mime = 'image/jpeg'),
 created_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX product_contributions_barcode_idx ON product_contributions(barcode, created_at DESC);
CREATE FUNCTION remove_orphan_community_product() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NOT EXISTS (SELECT 1 FROM product_contributions WHERE barcode=OLD.barcode) THEN
   DELETE FROM products WHERE barcode=OLD.barcode AND provider='community';
 END IF;
 RETURN NULL;
END $$;
CREATE TRIGGER contribution_removed AFTER DELETE ON product_contributions
 FOR EACH ROW EXECUTE FUNCTION remove_orphan_community_product();

CREATE TABLE saved_collections (
 id uuid PRIMARY KEY,
 user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
 name text NOT NULL CHECK(length(name) BETWEEN 1 AND 60),
 created_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX saved_collections_user_idx ON saved_collections(user_id, created_at);
CREATE TABLE saved_collection_products (
 collection_id uuid NOT NULL REFERENCES saved_collections(id) ON DELETE CASCADE,
 product_id text NOT NULL REFERENCES products(id) ON DELETE CASCADE,
 added_at timestamptz NOT NULL DEFAULT now(),
 PRIMARY KEY(collection_id, product_id)
);
