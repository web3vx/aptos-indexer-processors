-- Your SQL goes here
alter table multisig_contacts drop constraint pk_multisig_contacts;
alter table multisig_contacts add column id SERIAL PRIMARY KEY;
