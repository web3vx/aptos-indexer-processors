-- Your SQL goes here
CREATE TABLE owners_wallets (
    owner_address VARCHAR(255),
    wallet_address VARCHAR(255),
    created_at TIMESTAMP WITHOUT TIME ZONE DEFAULT (now() AT TIME ZONE 'utc'),
    PRIMARY KEY (owner_address, wallet_address),
    FOREIGN KEY (owner_address) REFERENCES multisig_owners(owner_address),
    FOREIGN KEY (wallet_address) REFERENCES multisig_wallets(wallet_address)
);

CREATE INDEX owner_address_index ON owners_wallets (owner_address);
CREATE INDEX wallet_address_index ON owners_wallets (wallet_address);