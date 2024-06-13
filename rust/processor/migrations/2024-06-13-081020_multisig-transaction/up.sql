-- Your SQL goes here
CREATE TABLE multisig_transactions (
    transaction_id VARCHAR(255) PRIMARY KEY,
    wallet_address VARCHAR(255) NOT NULL,
    initiated_by VARCHAR(255) NOT NULL,
    sequence_number integer NOT NULL,
    payload JSONB NOT NULL,
    status integer NOT NULL,
    created_at TIMESTAMP WITHOUT TIME ZONE DEFAULT (now() AT TIME ZONE 'utc'),
    FOREIGN KEY (wallet_address) REFERENCES multisig_wallets(wallet_address),
    FOREIGN KEY (initiated_by) REFERENCES multisig_owners(owner_address),
    CONSTRAINT unique_wallet_address_sequence_number UNIQUE (sequence_number, wallet_address)
);


CREATE INDEX initiated_by_index ON multisig_transactions (initiated_by);
CREATE INDEX sequence_number_index ON multisig_transactions (sequence_number);
CREATE INDEX status_index ON multisig_transactions (status);
