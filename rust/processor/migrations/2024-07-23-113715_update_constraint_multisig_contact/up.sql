-- Your SQL goes here
ALTER TABLE multisig_contacts RENAME COLUMN owner_address TO wallet_address;
ALTER TABLE multisig_contacts ADD CONSTRAINT multisig_contacts_uniq UNIQUE (wallet_address, contact_address);
ALTER TABLE multisig_contacts ADD CONSTRAINT fk_multisig_contacts FOREIGN KEY (wallet_address) REFERENCES multisig_wallets (wallet_address);