-- Your SQL goes here
ALTER TABLE owners_wallets DROP CONSTRAINT owners_wallets_owner_address_fkey;
alter table owners_wallets
    drop constraint owners_wallets_wallet_address_fkey;

