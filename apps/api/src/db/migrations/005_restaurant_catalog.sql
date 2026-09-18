CREATE EXTENSION IF NOT EXISTS postgis;
CREATE EXTENSION IF NOT EXISTS pg_trgm;

CREATE TABLE restaurant_catalog (
 id text PRIMARY KEY CHECK(id ~ '^osm_[nwr]_[0-9]+$'),
 region text NOT NULL,
 name text NOT NULL,
 address text NOT NULL DEFAULT '',
 category text NOT NULL,
 location geography(Point,4326) NOT NULL,
 website text,
 phone text,
 gluten_tag text,
 imported_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX restaurant_catalog_location_idx ON restaurant_catalog USING gist(location);
CREATE INDEX restaurant_catalog_name_idx ON restaurant_catalog USING gin(name gin_trgm_ops);
CREATE INDEX restaurant_catalog_category_idx ON restaurant_catalog(category);
CREATE INDEX restaurant_catalog_region_idx ON restaurant_catalog(region);

CREATE TABLE restaurant_catalog_imports (
 region text PRIMARY KEY,
 imported_at timestamptz NOT NULL DEFAULT now(),
 item_count integer NOT NULL CHECK(item_count > 0)
);
