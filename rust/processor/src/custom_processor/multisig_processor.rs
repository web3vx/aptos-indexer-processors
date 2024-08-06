// Copyright © Aptos Foundation
// SPDX-License-Identifier: Apache-2.0

use std::fmt::Debug;

use ahash::AHashMap;
use anyhow::bail;
use aptos_protos::transaction::v1::write_set_change::Change;
use aptos_protos::transaction::v1::{transaction::TxnData, Event, Transaction, WriteResource};
use async_trait::async_trait;
use chrono::{DateTime, NaiveDateTime, Utc};
use diesel::{
    pg::{upsert::excluded, Pg},
    query_builder::QueryFragment,
    BoolExpressionMethods, ExpressionMethods, QueryDsl,
};
use serde_json::{to_string, Value};
use tracing::error;
use tracing::log::info;

use crate::custom_processor::utils::utils::{
    decode_event_payload, parse_payload, process_entry_function,
};
use crate::custom_processor::{CustomProcessorName, CustomProcessorTrait};
use crate::models::multisig_transaction_models::multisig_transaction::{
    MultisigTransaction, TransactionStatus,
};
use crate::models::multisig_voting_transaction_models::multisig_voting_transaction::MultisigVotingTransaction;
use crate::processors::ProcessingResult;
use crate::schema::multisig_transactions::{
    executed_at, executor, payload, sequence_number, status, wallet_address,
};
use crate::schema::owners_wallets::owner_address;
use crate::schema::{ledger_infos, multisig_transactions};
use crate::utils::database::execute_with_better_error;
use crate::utils::util::{extract_multisig_wallet_data_from_write_resource, standardize_address};
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
    )
    .await?;
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
    )
    .await?;
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
    )
    .await?;
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
    )
    .await?;
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
    )
    .await?;
    Ok(())
}

async fn remove_owners_db(
    pool: &PgDbPool,
    owners: Vec<&str>,
    from_wallet_address: &str,
) -> Result<(), diesel::result::Error> {
    execute_with_better_error(
        pool.clone(),
        diesel::delete(schema::owners_wallets::table)
            .filter(owner_address.eq_any(owners))
            .filter(crate::schema::owners_wallets::wallet_address.eq(from_wallet_address)),
        None,
    )
    .await?;

    Ok(())
}

#[derive(AsChangeset)]
#[table_name = "multisig_transactions"]
struct UpdateTransaction<'a> {
    status: i32,
    executor: Option<&'a str>,
    executed_at: Option<NaiveDateTime>,
    payload: Option<Value>,
    error: Option<Value>,
}

async fn update_transaction_status(
    pool: &PgDbPool,
    filter_wallet_address: String,
    filter_sequence_number: i32,
    new_status: i32,
    new_executor: Option<String>,
    new_executed_at: Option<NaiveDateTime>,
    transaction_payload: &str,
) -> anyhow::Result<()> {
    let target = schema::multisig_transactions::table.filter(
        wallet_address
            .eq(filter_wallet_address)
            .and(sequence_number.eq(filter_sequence_number)),
    );

    let payload_value = serde_json::from_str(transaction_payload).unwrap_or_else(|_| Value::Null);

    let update = UpdateTransaction {
        status: new_status,
        executor: new_executor.as_deref(),
        executed_at: new_executed_at,
        payload: if payload_value.is_null() {
            None
        } else {
            Some(payload_value)
        },
        error: None,
    };

    execute_with_better_error(pool.clone(), diesel::update(target).set(update), None).await?;

    Ok(())
}

async fn update_failed_transaction_status(
    pool: &PgDbPool,
    filter_wallet_address: String,
    filter_sequence_number: i32,
    new_executor: Option<String>,
    new_executed_at: Option<NaiveDateTime>,
    error_payload: &str,
) -> anyhow::Result<()> {
    let target = schema::multisig_transactions::table.filter(
        wallet_address
            .eq(filter_wallet_address)
            .and(sequence_number.eq(filter_sequence_number)),
    );

    let error_value = serde_json::from_str(error_payload).unwrap_or_else(|_| Value::Null);

    let update = UpdateTransaction {
        status: TransactionStatus::Failed as i32,
        executor: new_executor.as_deref(),
        executed_at: new_executed_at,
        error: Some(error_value),
        payload: None,
    };

    let response =
        execute_with_better_error(pool.clone(), diesel::update(target).set(update), None).await;
    if response.is_err() {
        error!("Error updating transaction status: {:?}", response);
    }

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
        diesel::insert_into(schema::multisig_wallets::table)
            .values(multisig_wallet)
            .on_conflict(wallet_address)
            .do_update()
            .set((
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
        diesel::insert_into(schema::multisig_owners::table)
            .values(owners)
            .on_conflict(owner_address)
            .do_update()
            .set((created_at.eq(excluded(created_at)),)),
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
        diesel::insert_into(schema::owners_wallets::table)
            .values(owner_wallets)
            .on_conflict((wallet_address, owner_address))
            .do_nothing(),
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
        diesel::insert_into(schema::multisig_transactions::table)
            .values(transactions)
            .on_conflict((sequence_number, wallet_address))
            .do_update()
            .set((
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
        diesel::insert_into(schema::multisig_voting_transactions::table)
            .values(votes)
            .on_conflict((transaction_sequence, wallet_address, voter_address))
            .do_update()
            .set((
                created_at.eq(excluded(created_at)),
                value.eq(excluded(value)),
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
        start_version: u64,
        end_version: u64,
        _: Option<u64>,
    ) -> anyhow::Result<ProcessingResult> {
        let processing_start = std::time::Instant::now();
        let db_insertion_start = std::time::Instant::now();

        for txn in &transactions {
            info!("transactions version {:?}", txn.version);
            let txn_version = txn.version as i64;

            let txn_data = match txn.txn_data.as_ref() {
                Some(data) => data,
                None => {
                    tracing::warn!(
                        transaction_version = txn_version,
                        "Transaction data doesn't exist"
                    );
                    PROCESSOR_UNKNOWN_TYPE_COUNT
                        .with_label_values(&["MultisigProcessor"])
                        .inc();
                    continue;
                },
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
                    )
                    .await?;
                }
                for event in raw_event {
                    match event.type_str.as_str() {
                        "0x1::multisig_account::CreateTransactionEvent" => {
                            info!(
                                "CreateTransactionEvent: transactions version {:?}",
                                txn.version
                            );
                            let info = txn.clone().info.unwrap();
                            let hash = standardize_address(hex::encode(info.hash.as_slice()).as_str());
                            handle_create_transaction_event(
                                self,
                                event,
                                &hash,
                                txn.clone().timestamp.unwrap().seconds,
                            )
                            .await?;
                        },
                        "0x1::multisig_account::RemoveOwnersEvent" => {
                            info!("RemoveOwnersEvent: transactions version {:?}", txn.version);
                            handle_remove_owners(self, event).await?;
                        },
                        "0x1::multisig_account::AddOwnersEvent" => {
                            info!("RemoveOwnersEvent: transactions version {:?}", txn.version);
                            handle_add_owners(self, event, &self.per_table_chunk_sizes).await?;
                        },
                        "0x1::multisig_account::TransactionExecutionFailedEvent" => {
                            info!(
                                "TransactionExecutionFailedEvent: transactions version {:?}",
                                txn.version
                            );
                            handle_transaction_failed_event(
                                self,
                                event,
                                txn.clone().timestamp.unwrap().seconds,
                            )
                            .await?;
                        },
                        "0x1::multisig_account::ExecuteRejectedTransactionEvent"
                        | "0x1::multisig_account::TransactionExecutionSucceededEvent" => {
                            info!(
                                "Changes status transactions: transactions version {:?}",
                                txn.version
                            );
                            handle_transaction_status_event(
                                self,
                                event,
                                txn.clone().timestamp.unwrap().seconds,
                            )
                            .await?;
                        },
                        "0x1::multisig_account::VoteEvent" => {
                            info!("VoteEvent: transactions version {:?}", txn.version);
                            handle_vote_event(self, event, txn.clone().timestamp.unwrap().seconds)
                                .await?;
                        },
                        _ => {},
                    }
                }
            }
        }

        let last_transaction_timestamp = transactions.last().unwrap().timestamp.clone();
        let processing_duration_in_secs = processing_start.elapsed().as_secs_f64();
        let db_insertion_duration_in_secs = db_insertion_start.elapsed().as_secs_f64();

        Ok(ProcessingResult {
            start_version,
            end_version,
            processing_duration_in_secs,
            db_insertion_duration_in_secs,
            last_transaction_timestamp,
        })
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
        let (required_signatures, metadata, owner_addresses) =
            extract_multisig_wallet_data_from_write_resource(&write_resource.data);
        let multisig_wallet = MultisigWallet {
            wallet_address: write_resource.address.clone(),
            required_signatures: required_signatures as i32,
            metadata: Some(metadata),
            created_at: Utc::now().naive_utc(),
        };

        insert_multisig_wallet_to_db(&conn, &[multisig_wallet], per_table_chunk).await?;

        let owners = owner_addresses
            .iter()
            .map(|entry_owner_address| MultisigOwner {
                owner_address: entry_owner_address.clone(),
                created_at: Utc::now().naive_utc(),
            })
            .collect::<Vec<MultisigOwner>>();

        insert_multisig_owners_to_db(&conn, &owners, per_table_chunk).await?;

        let owner_wallets = owner_addresses
            .iter()
            .map(|entry_owner_address| OwnersWallet {
                owner_address: entry_owner_address.clone(),
                wallet_address: write_resource.address.clone(),
                created_at: Utc::now().naive_utc(),
            })
            .collect::<Vec<OwnersWallet>>();

        insert_to_owner_wallet_db(&conn, &owner_wallets, per_table_chunk).await?;
    }
    Ok(())
}

async fn handle_vote_event(
    processor: &MultisigProcessor,
    event: &Event,
    timestamp: i64,
) -> anyhow::Result<()> {
    let event_data: Value = serde_json::from_str(&event.data)?;

    let multisig_vote = MultisigVotingTransaction {
        wallet_address: standardize_address(event.key.as_ref().unwrap().account_address.as_str()),
        transaction_sequence: event_data["sequence_number"]
            .as_str()
            .unwrap_or("0")
            .parse::<i32>()?,
        voter_address: event_data["owner"].as_str().unwrap().to_string(),
        value: event_data["approved"].as_bool().unwrap(),
        created_at: DateTime::from_timestamp(timestamp, 0).unwrap().naive_utc(),
    };

    insert_to_votes_db(
        &processor.get_pool(),
        &[multisig_vote],
        &processor.per_table_chunk_sizes,
    )
    .await?;
    Ok(())
}

async fn handle_transaction_failed_event(
    processor: &MultisigProcessor,
    event: &Event,
    timestamp: i64,
) -> anyhow::Result<()> {
    info!("Processing Update Transaction Status {:?}", &event.data);
    let event_data: Value = serde_json::from_str(&event.data)?;
    let mut new_executor = None;
    let mut new_executed_at = None;
    let error_payload = event_data["execution_error"].clone();
    let filter_wallet_address =
        standardize_address(event.key.as_ref().unwrap().account_address.as_str());
    let filter_sequence_number = event_data["sequence_number"]
        .as_str()
        .unwrap_or("0")
        .parse::<i32>()?;

    if let Some(executor_str) = event_data["executor"].as_str() {
        new_executor = Some(executor_str.to_string());
    }
    new_executed_at = Some(DateTime::from_timestamp(timestamp, 0).unwrap().naive_utc());

    update_failed_transaction_status(
        &processor.get_pool(),
        filter_wallet_address,
        filter_sequence_number,
        new_executor,
        new_executed_at,
        &error_payload.to_string(),
    )
    .await?;

    Ok(())
}

async fn handle_transaction_status_event(
    processor: &MultisigProcessor,
    event: &Event,
    timestamp: i64,
) -> anyhow::Result<()> {
    info!("Processing Update Transaction Status {:?}", &event.data);
    let event_data: Value = serde_json::from_str(&event.data)?;
    let mut new_executor = None;
    let mut new_executed_at = None;
    let mut new_status: i32 = TransactionStatus::Pending as i32;
    let mut transaction_payload = String::from("");
    let filter_wallet_address =
        standardize_address(event.key.as_ref().unwrap().account_address.as_str());
    let filter_sequence_number = event_data["sequence_number"]
        .as_str()
        .unwrap_or("0")
        .parse::<i32>()?;

    match event.type_str.as_str() {
        "0x1::multisig_account::ExecuteRejectedTransactionEvent" => {
            new_status = TransactionStatus::Rejected as i32;
        },
        "0x1::multisig_account::TransactionExecutionSucceededEvent" => {
            transaction_payload = event_data["transaction_payload"]
                .as_str()
                .unwrap_or(&String::from(""))
                .to_string();
            new_status = TransactionStatus::Success as i32;
            let decoded_payload = hex::decode(
                transaction_payload
                    .strip_prefix("0x")
                    .unwrap_or(&transaction_payload),
            )
            .unwrap_or_default();

            if !decoded_payload.is_empty() {
                match parse_payload(&decoded_payload) {
                    Ok(multisig_transaction_payload) => {
                        transaction_payload = process_entry_function(&multisig_transaction_payload)
                            .await
                            .unwrap_or(Value::String(String::from("")))
                            .to_string();
                    },
                    Err(e) => {
                        tracing::warn!("Error parsing payload: {:?}", e);
                    },
                }
            }
        },

        _ => {},
    };
    if let Some(executor_str) = event_data["executor"].as_str() {
        new_executor = Some(executor_str.to_string());
    }
    new_executed_at = Some(DateTime::from_timestamp(timestamp, 0).unwrap().naive_utc());

    update_transaction_status(
        &processor.get_pool(),
        filter_wallet_address,
        filter_sequence_number,
        new_status,
        new_executor,
        new_executed_at,
        &transaction_payload,
    )
    .await?;

    Ok(())
}

async fn handle_create_transaction_event(
    processor: &MultisigProcessor,
    event: &Event,
    hash: &str,
    timestamp: i64,
) -> anyhow::Result<()> {
    info!("Processing CreateTransactionEvent {:?}", &event.data);
    let event_data: Value = serde_json::from_str(&event.data).unwrap_or_else(|_| {
        tracing::warn!("Failed to parse event data as JSON.");
        Value::Null
    });
    let mut json_payload = event_data["transaction"]["payload"].clone();
    let decoded_payload = decode_event_payload(&event_data).unwrap_or_default();

    if !decoded_payload.is_empty() {
        match parse_payload(&decoded_payload) {
            Ok(multisig_transaction_payload) => {
                json_payload = process_entry_function(&multisig_transaction_payload)
                    .await
                    .unwrap_or_else(|_| Value::Null);
            },
            Err(e) => {
                tracing::warn!("Error parsing payload: {:?}", e);
            },
        }
    }

    let multisig_transaction = MultisigTransaction {
        wallet_address: standardize_address(event.key.as_ref().unwrap().account_address.as_str()),
        sequence_number: event_data["sequence_number"]
            .as_str()
            .unwrap_or("0")
            .parse::<i32>()?,
        initiated_by: event_data["creator"].as_str().unwrap_or("").to_string(),
        payload: json_payload,
        payload_hash: Some(event_data["transaction"]["payload_hash"].clone()),
        created_at: DateTime::from_timestamp(timestamp, 0).unwrap().naive_utc(),
        status: TransactionStatus::Pending as i32,
        transaction_hash: Some(hash.to_string()),
        executor: None,
        executed_at: None,
    };
    info!("Custom Processing transactions: {:?}", multisig_transaction);
    insert_to_transaction_db(
        &processor.get_pool(),
        &[multisig_transaction],
        &processor.per_table_chunk_sizes,
    )
    .await?;
    process_votes(processor, event, &event_data, timestamp).await?;
    Ok(())
}

async fn process_votes(
    processor: &MultisigProcessor,
    event: &Event,
    event_data: &Value,
    timestamp: i64,
) -> anyhow::Result<()> {
    info!("Processing Vote Transaction {:?}", &event.data);

    let vote_array = event_data["transaction"]["votes"]["data"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("Votes data missing"))?;
    if let Some(first_vote) = vote_array.get(0) {
        let multisig_vote = MultisigVotingTransaction {
            wallet_address: standardize_address(
                event.key.as_ref().unwrap().account_address.as_str(),
            ),
            voter_address: standardize_address(first_vote["key"].as_str().unwrap()),
            transaction_sequence: event_data["sequence_number"]
                .as_str()
                .unwrap_or("0")
                .parse()?,
            value: first_vote["value"].as_bool().unwrap(),
            created_at: DateTime::from_timestamp(timestamp, 0).unwrap().naive_utc(),
        };
        insert_to_votes_db(
            &processor.get_pool(),
            &[multisig_vote],
            &processor.per_table_chunk_sizes,
        )
        .await?;
    }
    Ok(())
}

async fn handle_remove_owners(processor: &MultisigProcessor, event: &Event) -> anyhow::Result<()> {
    let event_data: Value = serde_json::from_str(&event.data)?;
    let owners_array = event_data["owners_removed"].as_array();
    if owners_array.is_some() {
        let owners = owners_array
            .unwrap()
            .iter()
            .map(|owner| owner.as_str().unwrap_or_default())
            .collect::<Vec<&str>>();

        let from_wallet_address =
            standardize_address(event.key.as_ref().unwrap().account_address.as_str());
        remove_owners_db(&processor.get_pool(), owners, &from_wallet_address).await?;
    }

    Ok(())
}

async fn handle_add_owners(
    processor: &MultisigProcessor,
    event: &Event,
    per_table_chunk_sizes: &AHashMap<String, usize>,
) -> anyhow::Result<()> {
    let event_data: Value = serde_json::from_str(&event.data)?;
    let from_wallet_address =
        standardize_address(event.key.as_ref().unwrap().account_address.as_str());
    let owner_wallets_str = event_data["owners_added"].as_array();
    if owner_wallets_str.is_some() {
        let owner_wallets = owner_wallets_str
            .unwrap()
            .iter()
            .map(|entry_owner_address| OwnersWallet {
                owner_address: entry_owner_address.as_str().unwrap_or("").to_string(),
                wallet_address: from_wallet_address.clone(),
                created_at: Utc::now().naive_utc(),
            })
            .collect::<Vec<OwnersWallet>>();

        insert_to_owner_wallet_db(&processor.get_pool(), &owner_wallets, per_table_chunk_sizes)
            .await?;
    }

    Ok(())
}
