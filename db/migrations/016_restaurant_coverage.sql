-- SafeEat 2.0.5: where the restaurant catalog has data, explicitly.
-- Before, "covered" meant "some venue within 15 km of the view's centre": a view half
-- over an area that was never imported (a border, the sea, another region) showed no
-- places there as if there were none. Additive: the 1.0.x API ignores this table.
--
-- Coverage is a grid of 0.1° cells (about 11 x 8.5 km in Spain), each cell with at
-- least one imported venue. No neighbour ring: measured on the Madrid import, a ring of
-- 0.25° cells reported Toledo, Guadalajara and Segovia (not imported, no venues) as
-- 100 % covered; 0.1° cells report them as 0 % and the region itself as 100 %. A rural
-- cell without any venue reads as "no data", which is the cautious side.
CREATE TABLE IF NOT EXISTS restaurant_catalog_coverage (
  region text NOT NULL REFERENCES restaurant_catalog_imports(region) ON DELETE CASCADE,
  cell_x integer NOT NULL,
  cell_y integer NOT NULL,
  cell geometry(Polygon, 4326) NOT NULL,
  PRIMARY KEY (region, cell_x, cell_y)
);
CREATE INDEX IF NOT EXISTS restaurant_catalog_coverage_cell_idx
  ON restaurant_catalog_coverage USING gist (cell);

CREATE OR REPLACE FUNCTION refresh_restaurant_coverage(target text) RETURNS integer
LANGUAGE plpgsql AS $$
DECLARE
  cells integer;
BEGIN
  DELETE FROM restaurant_catalog_coverage WHERE region = target;
  INSERT INTO restaurant_catalog_coverage (region, cell_x, cell_y, cell)
  SELECT target, v.x, v.y,
         ST_MakeEnvelope(v.x * 0.1, v.y * 0.1, (v.x + 1) * 0.1, (v.y + 1) * 0.1, 4326)
  FROM (
    SELECT DISTINCT floor(ST_X(location::geometry) / 0.1)::integer AS x,
                    floor(ST_Y(location::geometry) / 0.1)::integer AS y
    FROM restaurant_catalog WHERE region = target
  ) v
  ON CONFLICT DO NOTHING;
  GET DIAGNOSTICS cells = ROW_COUNT;
  RETURN cells;
END;
$$;

-- Regions imported before this migration.
SELECT refresh_restaurant_coverage(region) FROM restaurant_catalog_imports;
