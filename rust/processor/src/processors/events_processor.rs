// Copyright © Aptos Foundation
// SPDX-License-Identifier: Apache-2.0

use super::{ProcessingResult, ProcessorName, ProcessorTrait};
use crate::{
    models::events_models::events::EventModel,
    schema,
    utils::{
        counters::PROCESSOR_UNKNOWN_TYPE_COUNT,
        database::{execute_in_chunks, PgDbPool},
    },
};
use anyhow::bail;
use aptos_protos::transaction::v1::{transaction::TxnData, Transaction};
use async_trait::async_trait;
use diesel::{
    pg::{upsert::excluded, Pg},
    query_builder::QueryFragment,
    ExpressionMethods,
};
use field_count::FieldCount;
use std::fmt::Debug;
use tracing::error;

pub struct EventsProcessor {
    connection_pool: PgDbPool,
}

impl EventsProcessor {
    pub fn new(connection_pool: PgDbPool) -> Self {
        Self { connection_pool }
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
    events: Vec<EventModel>,
) -> Result<(), diesel::result::Error> {
    tracing::trace!(
        name = name,
        start_version = start_version,
        end_version = end_version,
        "Inserting to db",
    );
    execute_in_chunks(conn, insert_events_query, events, EventModel::field_count()).await?;
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
            let tnx_user_request =  match txn_data {
                TxnData::User(tx_inner) => &tx_inner.request,
                _ => &request_default,
            };
            //  If request is None, it means that the transaction is not a user transaction, skip
            if tnx_user_request.is_none() {
                continue;
            }
            let inserted_at = txn.timestamp.clone();

            let txn_events = EventModel::from_events(raw_events, txn_version, block_height, tnx_user_request, &inserted_at);
            let mut filtered_events = vec![];
            let filter_addresses = vec![
                "0xf9254492a5bb97685bb1789834668f3f8f391336b11c063b74ac6f83c37f6ecf::tapos_game_2::SetTaskPoints",
                "0xf9254492a5bb97685bb1789834668f3f8f391336b11c063b74ac6f83c37f6ecf::tapos_game_2::AddEquipment",
            ];
            for txn_event in txn_events {
                if filter_addresses.iter().any(|address| txn_event.type_.contains(address)) {
                    filtered_events.push(txn_event);
                }
            }

            events.extend(filtered_events);
        }

        let processing_duration_in_secs = processing_start.elapsed().as_secs_f64();
        let db_insertion_start = std::time::Instant::now();

        let tx_result = insert_to_db(
            self.get_pool(),
            self.name(),
            start_version,
            end_version,
            events,
        )
        .await;

        let db_insertion_duration_in_secs = db_insertion_start.elapsed().as_secs_f64();
        match tx_result {
            Ok(_) => Ok(ProcessingResult {
                start_version,
                end_version,
                processing_duration_in_secs,
                db_insertion_duration_in_secs,
                last_transaction_timstamp: transactions.last().unwrap().timestamp.clone(),
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
