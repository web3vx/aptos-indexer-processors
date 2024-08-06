-- Your SQL goes here
drop table if exists multisig_contacts;

CREATE TABLE multisig_contacts (
   id SERIAL PRIMARY KEY NOT NULL,
   wallet_address VARCHAR(255) NOT NULL,
   contact_address VARCHAR(255) NOT NULL,
   contact_name VARCHAR(255) NOT NULL,
   created_at TIMESTAMP DEFAULT NOW() NOT NULL
);

ALTER TABLE multisig_contacts ADD CONSTRAINT multisig_contacts_uniq UNIQUE (wallet_address, contact_address);
ALTER TABLE multisig_contacts ADD CONSTRAINT fk_multisig_contacts FOREIGN KEY (wallet_address) REFERENCES multisig_wallets (wallet_address);