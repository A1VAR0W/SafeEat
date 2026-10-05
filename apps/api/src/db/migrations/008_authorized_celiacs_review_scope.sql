CREATE TABLE roles (
 code text PRIMARY KEY,
 name text NOT NULL UNIQUE
);
INSERT INTO roles(code,name) VALUES('authorized_celiac','Celiacos autorizados');
CREATE TABLE user_roles (
 user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
 role_code text NOT NULL REFERENCES roles(code),
 assigned_at timestamptz NOT NULL DEFAULT now(),
 PRIMARY KEY(user_id,role_code)
);
CREATE INDEX restaurant_catalog_name_group_idx ON restaurant_catalog(restaurant_brand_name(name));
CREATE INDEX community_restaurants_name_group_idx ON community_restaurants(restaurant_brand_name(name));
ALTER TABLE restaurant_gluten_reports ADD COLUMN scope text NOT NULL DEFAULT 'local' CHECK(scope IN ('local','chain'));
ALTER TABLE restaurant_gluten_reports DISABLE TRIGGER restaurant_review_changed;
UPDATE restaurant_gluten_reports SET scope='chain' WHERE chain_key IS NOT NULL;
ALTER TABLE restaurant_gluten_reports ENABLE TRIGGER restaurant_review_changed;
ALTER TABLE restaurant_gluten_reports ADD CHECK ((scope='chain')=(chain_key IS NOT NULL));

CREATE OR REPLACE FUNCTION refresh_restaurant_chains() RETURNS void LANGUAGE plpgsql AS $$
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
   AND r.chain_key IS DISTINCT FROM c.chain_key AND r.scope='chain';
END $$;
