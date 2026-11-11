-- Existing accounts remain valid; only new registrations require profile fields.
ALTER TABLE users
 ADD COLUMN first_name text CHECK (first_name IS NULL OR length(first_name) BETWEEN 1 AND 80),
 ADD COLUMN last_name text CHECK (last_name IS NULL OR length(last_name) <= 120),
 ADD COLUMN date_of_birth date,
 ADD COLUMN product_updates_consent boolean NOT NULL DEFAULT false,
 ADD COLUMN product_updates_consent_at timestamptz,
 ADD COLUMN product_updates_consent_version text;
