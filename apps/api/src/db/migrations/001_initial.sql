CREATE TABLE users (
 id uuid PRIMARY KEY, email text NOT NULL UNIQUE, password_hash text NOT NULL,
 created_at timestamptz NOT NULL DEFAULT now(), updated_at timestamptz NOT NULL DEFAULT now()
);
CREATE TABLE refresh_sessions (
 id uuid PRIMARY KEY, user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
 family_id uuid NOT NULL, token_hash text NOT NULL UNIQUE, expires_at timestamptz NOT NULL,
 revoked_at timestamptz, created_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX refresh_sessions_user_idx ON refresh_sessions(user_id);
CREATE INDEX refresh_sessions_family_idx ON refresh_sessions(family_id);
CREATE INDEX refresh_sessions_expiration_idx ON refresh_sessions(expires_at);
CREATE TABLE products (
 id text PRIMARY KEY, barcode text UNIQUE, provider text NOT NULL,
 owner_id uuid REFERENCES users(id) ON DELETE CASCADE,
 data jsonb NOT NULL, fetched_at timestamptz NOT NULL DEFAULT now(), expires_at timestamptz NOT NULL
);
CREATE INDEX products_owner_idx ON products(owner_id);
CREATE TABLE favorite_products (
 user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
 product_id text NOT NULL REFERENCES products(id) ON DELETE CASCADE,
 created_at timestamptz NOT NULL DEFAULT now(), PRIMARY KEY(user_id, product_id)
);
-- Only Google place IDs are retained. Provider details are fetched on demand.
CREATE TABLE favorite_restaurants (
 user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
 restaurant_id text NOT NULL, created_at timestamptz NOT NULL DEFAULT now(), PRIMARY KEY(user_id, restaurant_id)
);
CREATE TABLE shares (
 token text PRIMARY KEY, kind text NOT NULL CHECK(kind IN ('product','restaurant')),
 resource_id text NOT NULL, owner_id uuid REFERENCES users(id) ON DELETE CASCADE,
 created_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX shares_owner_idx ON shares(owner_id);
CREATE INDEX shares_resource_idx ON shares(kind,resource_id);
