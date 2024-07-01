-- Your SQL goes here
ALTER TABLE multisig_voting_transactions DROP CONSTRAINT multisig_voting_transactions_voter_address_fkey;
alter table multisig_voting_transactions
    drop constraint multisig_voting_transactions_wallet_address_transaction_se_fkey;

