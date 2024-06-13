CREATE TABLE multisig_wallets (
    wallet_address VARCHAR(255) PRIMARY KEY,
    required_signatures INT NOT NULL CHECK (required_signatures > 0),
    metadata JSONB,
    created_at TIMESTAMP WITHOUT TIME ZONE DEFAULT (now() AT TIME ZONE 'utc')
);

CREATE INDEX created_at_index ON multisig_wallets (created_at);