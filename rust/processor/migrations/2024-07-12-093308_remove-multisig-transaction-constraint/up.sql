-- Your SQL goes here
ALTER TABLE multisig_transactions DROP CONSTRAINT multisig_transactions_initiated_by_fkey;
alter table multisig_transactions drop constraint multisig_transactions_wallet_address_fkey;

