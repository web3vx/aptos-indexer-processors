// Copyright © Aptos Foundation
// SPDX-License-Identifier: Apache-2.0

use super::{ProcessingResult, ProcessorName, ProcessorTrait};
use crate::utils::database::PgPoolConnection;
use crate::utils::util::{
    is_multisig_wallet_created_transaction, standardize_address, truncate_str,
};
use crate::{
    models::events_models::events::EventModel,
    schema,
    utils::{
        counters::PROCESSOR_UNKNOWN_TYPE_COUNT,
        database::{execute_in_chunks, get_config_table_chunk_size, PgDbPool},
    },
};
use ahash::AHashMap;
use anyhow::bail;
use aptos_protos::transaction::v1::write_set_change::Change;
use aptos_protos::transaction::v1::{
    transaction::TxnData, Event, EventKey, Transaction, WriteSetChange,
};
use aptos_protos::util::timestamp::Timestamp;
use async_trait::async_trait;
use diesel::{
    pg::{upsert::excluded, Pg},
    query_builder::QueryFragment,
    ExpressionMethods,
};
use once_cell::sync::Lazy;
use std::fmt::Debug;
use std::future::Future;
use std::pin::Pin;
use tracing::error;
use tracing::log::info;

static FILTERED_EVENTS: Lazy<Vec<&str>> = Lazy::new(|| {
    vec![
        "0x1::transaction_fee::FeeStatement",
        "0x1::multisig_account::create_with_owners",
    ]
});
static REQUIRED_EVENTS: Lazy<Vec<&str>> = Lazy::new(|| {
    vec![
        "0x111ae3e5bc816a5e63c2da97d0aa3886519e0cd5e4b046659fa35796bd11542a",
        "0x9770fa9c725cbd97eb50b2be5f7416efdfd1f1554beb0750d4dae4c64e860da3",
        "0x190d44266241744264b964a37b8f09863167a12d3e70cda39376cfb4e3561e12",
        "0x61d2c22a6cb7831bee0f48363b0eec92369357aece0d1142062f7d5d85c7bef8",
        "0xc7efb4076dbe143cbcd98cfaaa929ecfc8f299203dfff63b95ccb6bfe19850fa",
        "0x48271d39d0b05bd6efca2278f22277d6fcc375504f9839fd73f74ace240861af",
        "0x5ae6789dd2fec1a9ec9cccfb3acaf12e93d432f0a3a42c92fe1a9d490b7bbc06",
        "0x31a6675cbe84365bf2b0cbce617ece6c47023ef70826533bde5203d32171dc3c",
        "0xe11c12ec495f3989c35e1c6a0af414451223305b579291fc8f3d9d0575a23c26",
        "0x584b50b999c78ade62f8359c91b5165ff390338d45f8e55969a04e65d76258c9",
        "0xd520d8669b0a3de23119898dcdff3e0a27910db247663646ad18cf16e44c6f5",
        "0xc0deb00c405f84c85dc13442e305df75d1288100cdd82675695f6148c7ece51c",
        "0x17f1e926a81639e9557f4e4934df93452945ec30bc962e11351db59eb0d78c33",
        "0x1::voting",
        "0x1::aptos_governance",
        "0x1::delegation_pool",
        "0x05a97986a9d031c4567e15b797be516910cfcb4156312482efc6a19c0a30c948",
        "0xfaf4e633ae9eb31366c9ca24214231760926576c7b625313b3688b5e900731f6",
        "0x163df34fccbf003ce219d3f1d9e70d140b60622cb9dd47599c25fb2f797ba6e",
        "0x4bf51972879e3b95c4781a5cdcb9e1ee24ef483e7d22f2d903626f126df62bd1",
        "0x3c1d4a86594d681ff7e5d5a233965daeabdc6a15fe5672ceeda5260038857183",
        "0xc6bc659f1649553c1a3fa05d9727433dc03843baac29473c817d06d39e7621ba",
        "0x167f411fc5a678fb40d86e0af646fa8f62458b686ad8996215248447037af40c",
        "0x1::multisig_account::CreateTransactionEvent",
        "0x1::multisig_account::AddOwnersEvent",
        "0x1::multisig_account::RemoveOwnersEvent",
        "0x1::multisig_account::VoteEvent",
        "0x1::multisig_account::TransactionExecutionSucceededEvent",
        "0x1::multisig_account::TransactionExecutionFailedEvent",
        "0x1::multisig_account::ExecuteRejectedTransactionEvent",
        "0xccd1a84ccea93531d7f165b90134aa0415feb30e8757ab1632dac68c0055f5c2",
    ]
});
pub struct EventsProcessor {
    connection_pool: PgDbPool,
    per_table_chunk_sizes: AHashMap<String, usize>,
}

impl EventsProcessor {
    pub fn new(connection_pool: PgDbPool, per_table_chunk_sizes: AHashMap<String, usize>) -> Self {
        Self {
            connection_pool,
            per_table_chunk_sizes,
        }
    }
}

impl Debug for EventsProcessor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let state = &self.connection_pool.state();
        write!(
            f,
            "EventsProcessor {{ connections: {:?}  idle_connections: {:?} }}",
            state.connections, state.idle_connections
        )
    }
}

async fn insert_to_db(
    conn: PgDbPool,
    name: &'static str,
    start_version: u64,
    end_version: u64,
    events: &[EventModel],
    per_table_chunk_sizes: &AHashMap<String, usize>,
) -> Result<(), diesel::result::Error> {
    tracing::trace!(
        name = name,
        start_version = start_version,
        end_version = end_version,
        "Inserting to db",
    );
    execute_in_chunks(
        conn,
        insert_events_query,
        events,
        get_config_table_chunk_size::<EventModel>("events", per_table_chunk_sizes),
    )
    .await?;
    Ok(())
}

fn insert_events_query(
    items_to_insert: Vec<EventModel>,
) -> (
    impl QueryFragment<Pg> + diesel::query_builder::QueryId + Send,
    Option<&'static str>,
) {
    use schema::events::dsl::*;
    (
        diesel::insert_into(schema::events::table)
            .values(items_to_insert)
            .on_conflict((transaction_version, event_index))
            .do_update()
            .set((
                inserted_at.eq(excluded(inserted_at)),
                indexed_type.eq(excluded(indexed_type)),
            )),
        None,
    )
}

#[async_trait]
impl ProcessorTrait for EventsProcessor {
    fn name(&self) -> &'static str {
        ProcessorName::EventsProcessor.into()
    }

    async fn process_transactions(
        &self,
        transactions: Vec<Transaction>,
        start_version: u64,
        end_version: u64,
        _: Option<u64>,
    ) -> anyhow::Result<ProcessingResult> {
        let processing_start = std::time::Instant::now();
        let last_transaction_timestamp = transactions.last().unwrap().timestamp.clone();

        let mut events = vec![];
        for txn in &transactions {
            let txn_version = txn.version as i64;
            let block_height = txn.block_height as i64;
            let txn_data = match txn.txn_data.as_ref() {
                Some(data) => data,
                None => {
                    tracing::warn!(
                        transaction_version = txn_version,
                        "Transaction data doesn't exist"
                    );
                    PROCESSOR_UNKNOWN_TYPE_COUNT
                        .with_label_values(&["EventsProcessor"])
                        .inc();
                    continue;
                },
            };

            let default = vec![];
            let raw_events = match txn_data {
                TxnData::BlockMetadata(tx_inner) => &tx_inner.events,
                TxnData::Genesis(tx_inner) => &tx_inner.events,
                TxnData::User(tx_inner) => &tx_inner.events,
                _ => &default,
            };
            let request_default = None;
            let tnx_user_request = match txn_data {
                TxnData::User(tx_inner) => &tx_inner.request,
                _ => &request_default,
            };
            //  If request is None, it means that the transaction is not a user transaction, skip
            if tnx_user_request.is_none() {
                continue;
            }
            let inserted_at = txn.timestamp.clone();

            if let TxnData::User(txn_inner) = txn_data {
                let changes = &txn.clone().info.unwrap().changes;
                let filtered = changes.iter().filter(|c| {
                    let Change::WriteResource(write_resource) = &c.change.as_ref().unwrap() else {
                        return false;
                    };
                    write_resource.type_str.as_str() == "0x1::multisig_account::MultisigAccount"
                });
                filtered.for_each(|c| {
                    if let Change::WriteResource(write_resource) = &c.change.as_ref().unwrap() {
                        let from = tnx_user_request.as_ref().unwrap().sender.as_str();
                        let event = Event {
                            key: Some(EventKey {
                                account_address: standardize_address(from),
                                creation_number: txn_inner.clone().request.unwrap().sequence_number,
                            }),
                            sequence_number: txn_inner.clone().request.unwrap().sequence_number,
                            r#type: None,
                            type_str: write_resource.type_str.to_string(),
                            data: write_resource.data.to_string(),
                        };
                        let txn_create_multisig_event = EventModel::from_event(
                            &event,
                            txn_version,
                            block_height,
                            events.len() as i64,
                            tnx_user_request,
                            &inserted_at,
                        );
                        events.push(txn_create_multisig_event);
                    }
                });
            }
            let txn_events = EventModel::from_events(
                raw_events,
                txn_version,
                block_height,
                tnx_user_request,
                &inserted_at,
            );
            for txn_event in txn_events {
                if (!FILTERED_EVENTS.contains(&txn_event.type_.as_str())
                    || REQUIRED_EVENTS.contains(&txn_event.type_.as_str()))
                    && !FILTERED_EVENTS.contains(&txn_event.entry_function_id_str.as_str())
                {
                    events.push(txn_event);
                }
            }
        }

        let processing_duration_in_secs = processing_start.elapsed().as_secs_f64();
        let db_insertion_start = std::time::Instant::now();

        let tx_result = insert_to_db(
            self.get_pool(),
            self.name(),
            start_version,
            end_version,
            &events,
            &self.per_table_chunk_sizes,
        )
        .await;

        let db_insertion_duration_in_secs = db_insertion_start.elapsed().as_secs_f64();
        match tx_result {
            Ok(_) => Ok(ProcessingResult {
                start_version,
                end_version,
                processing_duration_in_secs,
                db_insertion_duration_in_secs,
                last_transaction_timestamp,
            }),
            Err(e) => {
                error!(
                    start_version = start_version,
                    end_version = end_version,
                    processor_name = self.name(),
                    error = ?e,
                    "[Parser] Error inserting transactions to db",
                );
                bail!(e)
            },
        }
    }

    fn connection_pool(&self) -> &PgDbPool {
        &self.connection_pool
    }
}
