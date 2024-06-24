-- Your SQL goes here
alter table multisig_transactions add  executor VARCHAR(255);
alter table multisig_transactions add  executed_at TIMESTAMP WITHOUT TIME ZONE;
