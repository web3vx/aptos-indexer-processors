-- Your SQL goes here
CREATE TABLE multisig_contacts (
  owner_address VARCHAR(255) NOT NULL,
  contact_address VARCHAR(255) NOT NULL,
  contact_name VARCHAR(255) NOT NULL,
  created_at TIMESTAMP DEFAULT NOW() NOT NULL,

  constraint pk_multisig_contacts primary key (owner_address, contact_address)
);