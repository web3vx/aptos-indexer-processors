-- Your SQL goes here
CREATE TABLE multisig_owners (
    owner_address VARCHAR(255) PRIMARY KEY,
    created_at TIMESTAMP WITHOUT TIME ZONE DEFAULT (now() AT TIME ZONE 'utc')
);