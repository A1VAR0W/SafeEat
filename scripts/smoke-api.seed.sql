INSERT INTO users(id,email,password_hash) VALUES
 ('11111111-1111-1111-1111-111111111111','admin@example.test','x'),
 ('22222222-2222-2222-2222-222222222222','pareja@example.test','x'),
 ('33333333-3333-3333-3333-333333333333','otro@example.test','x');
INSERT INTO roles(code,name) VALUES('ADMIN','Administradores') ON CONFLICT DO NOTHING;
INSERT INTO user_roles(user_id,role_code) VALUES
 ('11111111-1111-1111-1111-111111111111','ADMIN'),
 ('22222222-2222-2222-2222-222222222222','authorized_celiac');
INSERT INTO restaurant_catalog(id,region,name,address,category,location,chain_key,chain_name,gluten_tag) VALUES
 ('osm_n_1','madrid','Burger King','Gran Vía 1','fast_food',ST_SetSRID(ST_MakePoint(-3.7038,40.4168),4326)::geography,'Q177054','Burger King',NULL),
 ('osm_n_2','madrid','Burger King Gran Vía','Gran Vía 30','fast_food',ST_SetSRID(ST_MakePoint(-3.7050,40.4200),4326)::geography,NULL,NULL,NULL),
 ('osm_n_3','madrid','BURGER KING - Sol','Sol 2','fast_food',ST_SetSRID(ST_MakePoint(-3.7030,40.4169),4326)::geography,NULL,NULL,NULL),
 ('osm_n_4','barcelona','Burger King','Rambla 1','fast_food',ST_SetSRID(ST_MakePoint(2.17,41.38),4326)::geography,'Q177054','Burger King',NULL),
 ('osm_n_5','madrid','Rodilla Sol','Sol 5','cafe',ST_SetSRID(ST_MakePoint(-3.7031,40.4170),4326)::geography,NULL,'Rodilla',NULL),
 ('osm_n_6','madrid','Rodilla Callao','Callao 1','cafe',ST_SetSRID(ST_MakePoint(-3.7060,40.4200),4326)::geography,NULL,'Rodilla',NULL),
 ('osm_n_7','madrid','Rodilla Atocha','Atocha 1','cafe',ST_SetSRID(ST_MakePoint(-3.6900,40.4070),4326)::geography,NULL,'Rodilla',NULL),
 ('osm_n_8','madrid','Mesón ÁNGEL','Mayor 3','restaurant',ST_SetSRID(ST_MakePoint(-3.7100,40.4150),4326)::geography,NULL,NULL,'yes'),
 ('osm_n_9','madrid','Casa Pepe','Mayor 9','restaurant',ST_SetSRID(ST_MakePoint(-3.7110,40.4155),4326)::geography,NULL,NULL,NULL),
 ('osm_n_10','sevilla','Casa Pepe','Feria 1','restaurant',ST_SetSRID(ST_MakePoint(-5.99,37.39),4326)::geography,NULL,NULL,NULL),
 ('osm_n_11','madrid','Bar','Mayor 11','restaurant',ST_SetSRID(ST_MakePoint(-3.7111,40.4156),4326)::geography,NULL,'Bar',NULL);
INSERT INTO restaurant_catalog(id,region,name,address,category,location,chain_key,chain_name)
 SELECT 'osm_n_'||(100+g),'valencia','Burger King','Valencia '||g,'fast_food',ST_SetSRID(ST_MakePoint(-0.37+g*0.001,39.47),4326)::geography,'Q177054','Burger King' FROM generate_series(1,20) g;
INSERT INTO restaurant_catalog(id,region,name,address,category,location,chain_key,chain_name) VALUES
 ('osm_n_12','madrid','Rodilla Princesa','Princesa 1','cafe',ST_SetSRID(ST_MakePoint(-3.7150,40.4280),4326)::geography,NULL,'Rodilla'),
 ('osm_n_13','madrid','Rodilla Goya','Goya 1','cafe',ST_SetSRID(ST_MakePoint(-3.6750,40.4250),4326)::geography,NULL,'Rodilla'),
 ('osm_n_14','madrid','Bar Pepe','Mayor 20','bar',ST_SetSRID(ST_MakePoint(-3.7120,40.4157),4326)::geography,NULL,'Mahou'),
 ('osm_n_15','madrid','Bar Luis','Mayor 21','bar',ST_SetSRID(ST_MakePoint(-3.7121,40.4158),4326)::geography,NULL,'Mahou'),
 ('osm_n_16','madrid','Bar Ana','Mayor 22','bar',ST_SetSRID(ST_MakePoint(-3.7122,40.4159),4326)::geography,NULL,'Mahou'),
 ('osm_n_17','madrid','Bar Eva','Mayor 23','bar',ST_SetSRID(ST_MakePoint(-3.7123,40.4160),4326)::geography,NULL,'Mahou'),
 ('osm_n_18','madrid','Bar Sol','Mayor 24','bar',ST_SetSRID(ST_MakePoint(-3.7124,40.4161),4326)::geography,NULL,'Mahou');
INSERT INTO restaurant_catalog_imports(region,item_count) VALUES('madrid',16);
SELECT refresh_restaurant_chains();
-- Chain review by a non-trusted user (must be downgraded), and by the trusted one (kept).
INSERT INTO restaurant_gluten_reports(restaurant_id,user_id,support,note,chain_key,scope) VALUES
 ('osm_n_1','33333333-3333-3333-3333-333333333333','GLUTEN_FREE_OPTIONS','Tienen pan sin gluten en todos','Q177054','chain'),
 ('osm_n_4','22222222-2222-2222-2222-222222222222','GLUTEN_FREE_OPTIONS','Pan sin gluten envasado y freidora aparte','Q177054','chain'),
 ('osm_n_9','22222222-2222-2222-2222-222222222222','DEDICATED_GLUTEN_FREE','Todo sin gluten en este local concreto','name:casapepe','chain');
