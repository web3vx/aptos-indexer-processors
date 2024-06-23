-- Your SQL goes here
CREATE TABLE multisig_voting_transactions (
    voter_address VARCHAR(255) NOT NULL,
    wallet_address VARCHAR(255) NOT NULL,
    transaction_sequence integer NOT NULL,
    value BOOLEAN NOT NULL,
    created_at TIMESTAMP WITHOUT TIME ZONE DEFAULT (now() AT TIME ZONE 'utc'),
    foreign key (wallet_address, transaction_sequence) references multisig_transactions(wallet_address, sequence_number),
    foreign key (voter_address) references multisig_owners(owner_address),
    PRIMARY KEY (transaction_sequence, wallet_address, voter_address)
);
