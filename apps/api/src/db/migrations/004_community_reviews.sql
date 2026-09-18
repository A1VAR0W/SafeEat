CREATE TABLE product_gluten_reports (
 product_id text NOT NULL REFERENCES products(id) ON DELETE CASCADE,
 user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
 status text NOT NULL CHECK(status IN ('GLUTEN_FREE','CONTAINS_GLUTEN')),
 note text NOT NULL CHECK(length(note) BETWEEN 10 AND 500),
 updated_at timestamptz NOT NULL DEFAULT now(),
 PRIMARY KEY(product_id,user_id)
);
CREATE INDEX product_gluten_reports_product_idx ON product_gluten_reports(product_id);

CREATE TABLE community_restaurants (
 id text PRIMARY KEY CHECK(id ~ '^se_[a-f0-9]{32}$'),
 user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
 name text NOT NULL CHECK(length(name) BETWEEN 2 AND 160),
 address text NOT NULL CHECK(length(address) BETWEEN 5 AND 300),
 latitude double precision NOT NULL CHECK(latitude BETWEEN -90 AND 90),
 longitude double precision NOT NULL CHECK(longitude BETWEEN -180 AND 180),
 created_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX community_restaurants_location_idx ON community_restaurants(latitude,longitude);
CREATE INDEX community_restaurants_name_idx ON community_restaurants USING gin (to_tsvector('simple',name));

CREATE TABLE restaurant_gluten_reports (
 restaurant_id text NOT NULL,
 user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
 support text NOT NULL CHECK(support IN ('DEDICATED_GLUTEN_FREE','GLUTEN_FREE_OPTIONS','NO_GLUTEN_FREE_OPTIONS')),
 note text NOT NULL CHECK(length(note) BETWEEN 10 AND 500),
 updated_at timestamptz NOT NULL DEFAULT now(),
 PRIMARY KEY(restaurant_id,user_id)
);
CREATE INDEX restaurant_gluten_reports_restaurant_idx ON restaurant_gluten_reports(restaurant_id);
CREATE FUNCTION remove_community_restaurant_links() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 DELETE FROM restaurant_gluten_reports WHERE restaurant_id=OLD.id;
 DELETE FROM favorite_restaurants WHERE restaurant_id=OLD.id;
 DELETE FROM shares WHERE kind='restaurant' AND resource_id=OLD.id;
 RETURN OLD;
END $$;
CREATE TRIGGER community_restaurant_removed AFTER DELETE ON community_restaurants
 FOR EACH ROW EXECUTE FUNCTION remove_community_restaurant_links();
