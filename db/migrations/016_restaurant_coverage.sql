-- SafeEat 2.0.5: where the restaurant catalog has data, explicitly.
-- Before, "covered" meant "some venue within 15 km of the view's centre": a view half
-- over an area that was never imported (a border, the sea, another region) showed no
-- places there as if there were none. Additive: the 1.0.x API ignores this table.
--
-- Coverage is a grid of 0.25° cells (about 28 x 20 km in Spain): every cell with an
-- imported venue, plus its eight neighbours, so a village's countryside counts as
-- covered while the sea or a country that was not imported does not.
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
  SELECT DISTINCT target, v.x + dx, v.y + dy,
         ST_MakeEnvelope((v.x + dx) * 0.25, (v.y + dy) * 0.25,
                         (v.x + dx + 1) * 0.25, (v.y + dy + 1) * 0.25, 4326)
  FROM (
    SELECT DISTINCT floor(ST_X(location::geometry) / 0.25)::integer AS x,
                    floor(ST_Y(location::geometry) / 0.25)::integer AS y
    FROM restaurant_catalog WHERE region = target
  ) v
  CROSS JOIN generate_series(-1, 1) dx
  CROSS JOIN generate_series(-1, 1) dy
  ON CONFLICT DO NOTHING;
  GET DIAGNOSTICS cells = ROW_COUNT;
  RETURN cells;
END;
$$;

-- Regions imported before this migration.
SELECT refresh_restaurant_coverage(region) FROM restaurant_catalog_imports;
