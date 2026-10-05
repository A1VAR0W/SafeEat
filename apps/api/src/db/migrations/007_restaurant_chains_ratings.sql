-- Exact aliases learned from explicit brand identities, never fuzzy restaurant names.
CREATE FUNCTION restaurant_brand_name(value text) RETURNS text
LANGUAGE sql IMMUTABLE STRICT PARALLEL SAFE AS $$
 SELECT regexp_replace(lower(translate(value,'áéíóúüñàèìòù','aeiouunaeiou')),'[^a-z0-9]','','g')
$$;
CREATE TABLE restaurant_chain_aliases (
 alias text PRIMARY KEY,
 chain_key text NOT NULL,
 chain_name text NOT NULL
);
CREATE FUNCTION refresh_restaurant_chains() RETURNS void LANGUAGE plpgsql AS $$
BEGIN
 DELETE FROM restaurant_chain_aliases;
 INSERT INTO restaurant_chain_aliases(alias,chain_key,chain_name)
 SELECT restaurant_brand_name(chain_name),min(chain_key),min(chain_name)
 FROM restaurant_catalog
 WHERE chain_key ~ '^Q[0-9]+$' AND chain_name IS NOT NULL
   AND length(restaurant_brand_name(chain_name))>=3
 GROUP BY restaurant_brand_name(chain_name) HAVING count(DISTINCT chain_key)=1;
 UPDATE restaurant_catalog c SET chain_key=a.chain_key,chain_name=a.chain_name
 FROM restaurant_chain_aliases a
 WHERE c.chain_key IS NULL AND a.alias=restaurant_brand_name(
   COALESCE(NULLIF(c.chain_name,''),regexp_replace(c.name,'\s+[-–·]\s+.*$','')));
 UPDATE restaurant_gluten_reports r SET chain_key=c.chain_key
 FROM restaurant_catalog c WHERE r.restaurant_id=c.id AND c.chain_key IS NOT NULL
   AND r.chain_key IS DISTINCT FROM c.chain_key
   AND r.support IN ('DEDICATED_GLUTEN_FREE','GLUTEN_FREE_OPTIONS');
END $$;
SELECT refresh_restaurant_chains();

CREATE TABLE restaurant_ratings (
 restaurant_id text NOT NULL,
 user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
 stars smallint NOT NULL CHECK(stars BETWEEN 1 AND 5),
 updated_at timestamptz NOT NULL DEFAULT now(),
 PRIMARY KEY(restaurant_id,user_id)
);
CREATE FUNCTION remove_community_restaurant_ratings() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 DELETE FROM restaurant_ratings WHERE restaurant_id=OLD.id;
 RETURN OLD;
END $$;
CREATE TRIGGER community_restaurant_ratings_removed AFTER DELETE ON community_restaurants
 FOR EACH ROW EXECUTE FUNCTION remove_community_restaurant_ratings();
