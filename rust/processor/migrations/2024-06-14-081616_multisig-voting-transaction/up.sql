-- Your SQL goes here
CREATE TABLE multisig_voting_transactions (
    wallet_address VARCHAR(255) NOT NULL,
    owner_address VARCHAR(255) NOT NULL,
    transaction_sequence integer NOT NULL,
    value BOOLEAN NOT NULL,
    created_at TIMESTAMP WITHOUT TIME ZONE DEFAULT (now() AT TIME ZONE 'utc'),
    foreign key (wallet_address) references multisig_wallets(wallet_address),
    foreign key (owner_address) references multisig_owners(owner_address),
    PRIMARY KEY (transaction_sequence, wallet_address, owner_address, value)
);