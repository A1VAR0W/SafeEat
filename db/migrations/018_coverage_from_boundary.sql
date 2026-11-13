-- SafeEat 2.0.5: coverage from the boundary of the imported extract.
-- 016 marked a 0.1° cell as covered when at least one venue was imported there, so a
-- single bar made a whole cell "covered". An import now records the boundary of its
-- OpenStreetMap extract (Geofabrik's .poly): a cell is covered when it lies inside an
-- area whose data was actually loaded, clipped to that boundary (no border or sea
-- cell spills over). Imports without a boundary keep 016's cautious rule.
-- Additive: a new nullable column, the cell column widened from Polygon to any
-- geometry (the old API only reads it with ST_Intersects / ST_Union), and the
-- refresh function replaced.

ALTER TABLE restaurant_catalog_imports
  ADD COLUMN IF NOT EXISTS area geometry(MultiPolygon, 4326);

ALTER TABLE restaurant_catalog_coverage
  ALTER COLUMN cell TYPE geometry(Geometry, 4326);

CREATE OR REPLACE FUNCTION refresh_restaurant_coverage(target text) RETURNS integer
LANGUAGE plpgsql AS $$
DECLARE
  cells integer;
  boundary geometry;
BEGIN
  SELECT area INTO boundary FROM restaurant_catalog_imports WHERE region = target;
  DELETE FROM restaurant_catalog_coverage WHERE region = target;
  IF boundary IS NULL THEN
    INSERT INTO restaurant_catalog_coverage (region, cell_x, cell_y, cell)
    SELECT target, v.x, v.y,
           ST_MakeEnvelope(v.x * 0.1, v.y * 0.1, (v.x + 1) * 0.1, (v.y + 1) * 0.1, 4326)
    FROM (
      SELECT DISTINCT floor(ST_X(location::geometry) / 0.1)::integer AS x,
                      floor(ST_Y(location::geometry) / 0.1)::integer AS y
      FROM restaurant_catalog WHERE region = target
    ) v
    ON CONFLICT DO NOTHING;
  ELSE
    INSERT INTO restaurant_catalog_coverage (region, cell_x, cell_y, cell)
    SELECT target, c.x, c.y, c.clipped
    FROM (
      SELECT g.x, g.y,
             ST_CollectionExtract(ST_Intersection(g.env, boundary), 3) AS clipped
      FROM (
        SELECT x, y, ST_MakeEnvelope(x * 0.1, y * 0.1, (x + 1) * 0.1, (y + 1) * 0.1, 4326) AS env
        FROM generate_series(floor(ST_XMin(boundary) / 0.1)::integer,
                             floor(ST_XMax(boundary) / 0.1)::integer) AS x,
             generate_series(floor(ST_YMin(boundary) / 0.1)::integer,
                             floor(ST_YMax(boundary) / 0.1)::integer) AS y
      ) g
      WHERE ST_Intersects(g.env, boundary)
    ) c
    WHERE NOT ST_IsEmpty(c.clipped)
    ON CONFLICT DO NOTHING;
  END IF;
  GET DIAGNOSTICS cells = ROW_COUNT;
  RETURN cells;
END;
$$;
