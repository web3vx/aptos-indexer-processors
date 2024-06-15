// Copyright © Aptos Foundation
// SPDX-License-Identifier: Apache-2.0

use std::fmt::Debug;

use ahash::AHashMap;
use aptos_protos::transaction::v1::{Event, Transaction, transaction::TxnData, WriteResource};
use aptos_protos::transaction::v1::write_set_change::Change;
use async_trait::async_trait;
use chrono::Utc;
use diesel::{BoolExpressionMethods, ExpressionMethods, pg::{Pg, upsert::excluded}, query_builder::QueryFragment, RunQueryDsl};
use serde_json::Value;
use tracing::log::info;

use crate::{
    models::multisig_owner_models::multisig_owner::MultisigOwner,
    models::multisig_owner_wallet_models::multisig_owner_wallet::OwnersWallet,
    models::multisig_wallet_models::multisig_wallet::MultisigWallet,
    schema,
    utils::{
        counters::PROCESSOR_UNKNOWN_TYPE_COUNT,
        database::{execute_in_chunks, get_config_table_chunk_size, PgDbPool},
    },
};
use crate::custom_processor::{CustomProcessorName, CustomProcessorTrait};
use crate::models::ledger_info::LedgerInfo;
use crate::models::multisig_transaction_models::multisig_transaction::MultisigTransaction;
use crate::models::multisig_voting_transaction_models::multisig_voting_transaction::MultisigVotingTransaction;
use crate::schema::ledger_infos;
use crate::schema::multisig_transactions::{sequence_number, status, wallet_address};
use crate::utils::database::execute_with_better_error;
use crate::utils::util::{
    extract_multisig_wallet_data_from_write_resource,
    standardize_address,
};

pub struct MultisigProcessor {
    connection_pool: PgDbPool,
    per_table_chunk_sizes: AHashMap<String, usize>,
}

impl MultisigProcessor {
    pub fn new(connection_pool: PgDbPool, per_table_chunk_sizes: AHashMap<String, usize>) -> Self {
        Self {
            connection_pool,
            per_table_chunk_sizes,
        }
    }
}

impl Debug for MultisigProcessor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let state = &self.connection_pool.state();
        write!(
            f,
            "MultisigProcessor {{ connections: {:?}  idle_connections: {:?} }}",
            state.connections, state.idle_connections
        )
    }
}

async fn insert_multisig_wallet_to_db(
    conn: &PgDbPool,
    multisig_wallets: &[MultisigWallet],
    per_table_chunk_sizes: &AHashMap<String, usize>,
) -> Result<(), diesel::result::Error> {
    execute_in_chunks(
        conn.clone(),
        insert_multisig_wallet_query,
        multisig_wallets,
        get_config_table_chunk_size::<MultisigWallet>("multisig_wallets", per_table_chunk_sizes),
    ).await?;
    Ok(())
}

async fn insert_multisig_owners_to_db(
    conn: &PgDbPool,
    owners: &[MultisigOwner],
    per_table_chunk_sizes: &AHashMap<String, usize>,
) -> Result<(), diesel::result::Error> {
    execute_in_chunks(
        conn.clone(),
        insert_multisig_owner_query,
        owners,
        get_config_table_chunk_size::<MultisigOwner>("multisig_owners", per_table_chunk_sizes),
    ).await?;
    Ok(())
}

async fn insert_to_owner_wallet_db(
    conn: &PgDbPool,
    owner_wallets: &[OwnersWallet],
    per_table_chunk_sizes: &AHashMap<String, usize>,
) -> Result<(), diesel::result::Error> {
    execute_in_chunks(
        conn.clone(),
        insert_multisig_owner_wallet_query,
        owner_wallets,
        get_config_table_chunk_size::<OwnersWallet>("owners_wallets", per_table_chunk_sizes),
    ).await?;
    Ok(())
}

async fn insert_to_transaction_db(
    conn: &PgDbPool,
    transactions: &[MultisigTransaction],
    per_table_chunk_sizes: &AHashMap<String, usize>,
) -> Result<(), diesel::result::Error> {
    execute_in_chunks(
        conn.clone(),
        insert_transaction_query,
        transactions,
        get_config_table_chunk_size::<MultisigTransaction>(
            "multisig_transactions",
            per_table_chunk_sizes,
        ),
    ).await?;
    Ok(())
}

async fn insert_to_votes_db(
    conn: &PgDbPool,
    votes: &[MultisigVotingTransaction],
    per_table_chunk_sizes: &AHashMap<String, usize>,
) -> Result<(), diesel::result::Error> {
    execute_in_chunks(
        conn.clone(),
        insert_multisig_voting_transaction_query,
        votes,
        get_config_table_chunk_size::<MultisigVotingTransaction>(
            "multisig_voting_transactions",
            per_table_chunk_sizes,
        ),
    ).await?;
    Ok(())
}

async fn update_transaction_status(
    pool: &PgDbPool,
    filter_wallet_address: String,
    filter_sequence_number: i32,
    new_status: i32,
) -> anyhow::Result<()> {
    execute_with_better_error(
        pool.clone(),
        diesel::update(schema::multisig_transactions::table).set((
            status.eq(new_status),
        )).filter(wallet_address.eq(filter_wallet_address).and(sequence_number.eq(filter_sequence_number))),
        None,
    ).await?;
    Ok(())
}

fn insert_multisig_wallet_query(
    multisig_wallet: Vec<MultisigWallet>,
) -> (
    impl QueryFragment<Pg> + diesel::query_builder::QueryId + Send,
    Option<&'static str>,
) {
    use schema::multisig_wallets::dsl::*;
    (
        diesel::insert_into(schema::multisig_wallets::table).values(multisig_wallet).on_conflict(wallet_address).do_update().set((
            required_signatures.eq(excluded(required_signatures)),
            created_at.eq(excluded(created_at)),
        )),
        None,
    )
}

fn insert_multisig_owner_query(
    owners: Vec<MultisigOwner>,
) -> (
    impl QueryFragment<Pg> + diesel::query_builder::QueryId + Send,
    Option<&'static str>,
) {
    use schema::multisig_owners::dsl::*;
    (
        diesel::insert_into(schema::multisig_owners::table).values(owners).on_conflict(owner_address).do_update().set((created_at.eq(excluded(created_at)), )),
        None,
    )
}

fn insert_multisig_owner_wallet_query(
    owner_wallets: Vec<OwnersWallet>,
) -> (
    impl QueryFragment<Pg> + diesel::query_builder::QueryId + Send,
    Option<&'static str>,
) {
    use schema::owners_wallets::dsl::*;
    (
        diesel::insert_into(schema::owners_wallets::table).values(owner_wallets).on_conflict((wallet_address, owner_address)).do_nothing(),
        None,
    )
}

fn insert_transaction_query(
    transactions: Vec<MultisigTransaction>,
) -> (
    impl QueryFragment<Pg> + diesel::query_builder::QueryId + Send,
    Option<&'static str>,
) {
    use schema::multisig_transactions::dsl::*;
    (
        diesel::insert_into(schema::multisig_transactions::table).values(transactions).on_conflict((sequence_number, wallet_address)).do_update().set((
            created_at.eq(excluded(created_at)),
            payload.eq(excluded(payload)),
            status.eq(excluded(status)),
        )),
        None,
    )
}

fn insert_multisig_voting_transaction_query(
    votes: Vec<MultisigVotingTransaction>,
) -> (
    impl QueryFragment<Pg> + diesel::query_builder::QueryId + Send,
    Option<&'static str>,
) {
    use schema::multisig_voting_transactions::dsl::*;
    (
        diesel::insert_into(schema::multisig_voting_transactions::table).values(votes).on_conflict((transaction_sequence, wallet_address, value)).do_update().set((
            created_at.eq(excluded(created_at)),
        )),
        None,
    )
}

#[async_trait]
impl CustomProcessorTrait for MultisigProcessor {
    fn name(&self) -> &'static str {
        CustomProcessorName::MultisigProcessor.into()
    }

    async fn process_transactions(
        &self,
        transactions: Vec<Transaction>,
        _start_version: u64,
        _end_version: u64,
        _: Option<u64>,
    ) -> anyhow::Result<()> {
        info!("Custom Processing transactions",);

        for txn in &transactions {
            let txn_version = txn.version as i64;
            let txn_data = match txn.txn_data.as_ref() {
                Some(data) => data,
                None => {
                    tracing::warn!(
                        transaction_version = txn_version,
                        "Transaction data doesn't exist"
                    );
                    PROCESSOR_UNKNOWN_TYPE_COUNT.with_label_values(&["MultisigProcessor"]).inc();
                    continue;
                }
            };

            let request_default = None;
            let tnx_user_request = match txn_data {
                TxnData::User(tx_inner) => &tx_inner.request,
                _ => &request_default,
            };
            if tnx_user_request.is_none() {
                continue;
            }

            if let TxnData::User(txn_inner) = txn_data {
                let raw_event = &txn_inner.events;
                for change in &txn.clone().info.unwrap().changes {
                    let Change::WriteResource(write_resource) = &change.change.as_ref().unwrap()
                        else {
                            continue;
                        };
                    process_write_resource(
                        self.get_pool(),
                        write_resource,
                        &self.per_table_chunk_sizes,
                    ).await?;
                }

                for event in raw_event {
                    match event.type_str.as_str() {
                        "0x1::multisig_account::CreateTransactionEvent" => {
                            handle_create_transaction_event(self, event).await?;
                        }
                        "0x1::multisig_account::ExecuteRejectedTransactionEvent" | "0x1::multisig_account::TransactionExecutionSucceededEvent" | "0x1::multisig_account::TransactionExecutionFailedEvent" => {
                            eprintln!("Transaction status event {:?}",event.type_str.as_str());
                            handle_transaction_status_event(self, event).await?;
                        }
                        "0x1::multisig_account::VoteEvent" => {
                            handle_vote_event(self, event).await?;
                        }
                        _ => {}
                    }
                }
            }
        }
        Ok(())
    }

    fn connection_pool(&self) -> &PgDbPool {
        &self.connection_pool
    }
}

async fn process_write_resource(
    conn: PgDbPool,
    write_resource: &WriteResource,
    per_table_chunk: &AHashMap<String, usize>,
) -> anyhow::Result<()> {
    if write_resource.type_str.as_str() == "0x1::multisig_account::MultisigAccount" {
        let (required_signatures, metadata, owner_addresses) = extract_multisig_wallet_data_from_write_resource(&write_resource.data);
        let multisig_wallet = MultisigWallet {
            wallet_address: write_resource.address.clone(),
            required_signatures: required_signatures as i32,
            metadata: Some(metadata),
            created_at: Utc::now().naive_utc(),
        };

        insert_multisig_wallet_to_db(&conn, &[multisig_wallet], per_table_chunk).await?;

        let owners = owner_addresses.iter().map(|entry_owner_address| MultisigOwner {
            owner_address: entry_owner_address.to_string(),
            created_at: Utc::now().naive_utc(),
        }).collect::<Vec<MultisigOwner>>();

        insert_multisig_owners_to_db(&conn, &owners, per_table_chunk).await?;

        let owner_wallets = owner_addresses.iter().map(|entry_owner_address| OwnersWallet {
            owner_address: entry_owner_address.to_string(),
            wallet_address: write_resource.address.clone(),
            created_at: Utc::now().naive_utc(),
        }).collect::<Vec<OwnersWallet>>();

        insert_to_owner_wallet_db(&conn, &owner_wallets, per_table_chunk).await?;
    }
    Ok(())
}

async fn handle_vote_event(processor: &MultisigProcessor, event: &Event) -> anyhow::Result<()> {
    let event_data: Value = serde_json::from_str(&event.data)?;

    let multisig_vote = MultisigVotingTransaction {
        wallet_address: standardize_address(event.key.as_ref().unwrap().account_address.as_str()),
        transaction_sequence: event_data["sequence_number"].as_str().unwrap_or("0").parse::<i32>()?,
        value: event_data["approved"].as_bool().unwrap(),
        created_at: Utc::now().naive_utc(),
    };

    insert_to_votes_db(
        &processor.get_pool(),
        &[multisig_vote],
        &processor.per_table_chunk_sizes,
    ).await?;
    Ok(())
}

async fn handle_transaction_status_event(
    processor: &MultisigProcessor,
    event: &Event,
) -> anyhow::Result<()> {
    let event_data: Value = serde_json::from_str(&event.data)?;
    let new_status: i32 = match event.type_str.as_str() {
        "0x1::multisig_account::ExecuteRejectedTransactionEvent" => 2,
        "0x1::multisig_account::TransactionExecutionSucceededEvent" => 3,
        "0x1::multisig_account::TransactionExecutionFailedEvent" => 4,
        _ => 0,
    };

    update_transaction_status(
        &processor.get_pool(),
        standardize_address(event.key.as_ref().unwrap().account_address.as_str()),
        event_data["sequence_number"].as_str().unwrap_or("0").parse::<i32>()?,
        new_status,
    ).await?;

    Ok(())
}

async fn handle_create_transaction_event(
    processor: &MultisigProcessor,
    event: &Event,
) -> anyhow::Result<()> {
    let event_data: Value = serde_json::from_str(&event.data)?;

    let multisig_transaction = MultisigTransaction {
        wallet_address: standardize_address(event.key.as_ref().unwrap().account_address.as_str()),
        sequence_number: event_data["sequence_number"].as_str().unwrap_or("0").parse::<i32>()?,
        initiated_by: event_data["creator"].as_str().unwrap_or("").to_string(),
        payload: event_data["transaction"]["payload"].clone(),
        payload_hash: Some(event_data["transaction"]["payload_hash"].clone()),
        created_at: Utc::now().naive_utc(),
        status: 0,
    };

    insert_to_transaction_db(
        &processor.get_pool(),
        &[multisig_transaction],
        &processor.per_table_chunk_sizes,
    ).await?;

    let vote_array = event_data["transaction"]["votes"]["data"].as_array().unwrap();
    if let Some(first_vote) = vote_array.get(0) {
        let multisig_vote = MultisigVotingTransaction {
            wallet_address: standardize_address(
                event.key.as_ref().unwrap().account_address.as_str(),
            ),
            transaction_sequence: event_data["sequence_number"].as_str().unwrap_or("0").parse::<i32>()?,
            value: first_vote["value"].as_bool().unwrap(),
            created_at: Utc::now().naive_utc(),
        };

        insert_to_votes_db(
            &processor.get_pool(),
            &[multisig_vote],
            &processor.per_table_chunk_sizes,
        ).await?;
    }

    Ok(())
}
