-- Restaurant page details from the venue's own OpenStreetMap tags. Additive: rows
-- imported before have NULL until the next catalog import.
--   cuisine: first `cuisine` value (pizza, spanish, ...).
--   photo:   a Wikimedia Commons file name (from `wikimedia_commons`, or an `image`
--            URL pointing to Commons). Other image hosts are not stored: their licence
--            is unknown.
ALTER TABLE restaurant_catalog ADD COLUMN IF NOT EXISTS cuisine text;
ALTER TABLE restaurant_catalog ADD COLUMN IF NOT EXISTS photo text;
