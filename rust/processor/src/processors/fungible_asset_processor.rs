// Copyright © Aptos Foundation
// SPDX-License-Identifier: Apache-2.0

use super::{DefaultProcessingResult, ProcessorName, ProcessorTrait};
use crate::{
    db::common::models::coin_models::{
        coin_activities::CoinActivity, coin_balances::CoinBalance, coin_infos::CoinInfo,
    },
    gap_detectors::ProcessingResult,
    schema,
    utils::database::{execute_in_chunks, ArcDbPool},
    worker::TableFlags,
};
use ahash::AHashMap;
use anyhow::bail;
use aptos_protos::transaction::v1::Transaction;
use async_trait::async_trait;
use diesel::{
    pg::{upsert::excluded, Pg},
    query_builder::QueryFragment,
    ExpressionMethods,
};
use field_count::FieldCount;
use std::collections::HashMap;
use std::fmt::Debug;
use tracing::error;

pub struct FungibleAssetProcessor {
    connection_pool: ArcDbPool,
    per_table_chunk_sizes: AHashMap<String, usize>,
    deprecated_tables: TableFlags,
}

impl FungibleAssetProcessor {
    pub fn new(
        connection_pool: ArcDbPool,
        per_table_chunk_sizes: AHashMap<String, usize>,
        deprecated_tables: TableFlags,
    ) -> Self {
        Self {
            connection_pool,
            per_table_chunk_sizes,
            deprecated_tables,
        }
    }
}

impl Debug for FungibleAssetProcessor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let state = &self.connection_pool.state();
        write!(
            f,
            "FungibleAssetTransactionProcessor {{ connections: {:?}  idle_connections: {:?} }}",
            state.connections, state.idle_connections
        )
    }
}

async fn insert_to_db(
    conn: ArcDbPool,
    name: &'static str,
    start_version: u64,
    end_version: u64,
    coin_activities: &[CoinActivity],
    coin_infos: &[CoinInfo],
    coin_balances: &[CoinBalance],
) -> Result<(), diesel::result::Error> {
    tracing::trace!(
        name = name,
        start_version = start_version,
        end_version = end_version,
        "Inserting to db",
    );

    execute_in_chunks(
        conn.clone(),
        insert_coin_activities_query,
        coin_activities,
        CoinActivity::field_count(),
    )
    .await?;
    execute_in_chunks(
        conn.clone(),
        insert_coin_infos_query,
        coin_infos,
        CoinInfo::field_count(),
    )
    .await?;
    execute_in_chunks(
        conn.clone(),
        insert_coin_balances_query,
        coin_balances,
        CoinBalance::field_count(),
    )
    .await?;
    Ok(())
}

fn insert_coin_activities_query(
    items_to_insert: Vec<CoinActivity>,
) -> (
    impl QueryFragment<Pg> + diesel::query_builder::QueryId + Send,
    Option<&'static str>,
) {
    use schema::coin_activities::dsl::*;

    (
        diesel::insert_into(schema::coin_activities::table)
            .values(items_to_insert)
            .on_conflict((
                transaction_version,
                event_account_address,
                event_creation_number,
                event_sequence_number,
            ))
            .do_update()
            .set((
                entry_function_id_str.eq(excluded(entry_function_id_str)),
                inserted_at.eq(excluded(inserted_at)),
            )),
        None,
    )
}

fn insert_coin_infos_query(
    items_to_insert: Vec<CoinInfo>,
) -> (
    impl QueryFragment<Pg> + diesel::query_builder::QueryId + Send,
    Option<&'static str>,
) {
    use schema::coin_infos::dsl::*;

    (
        diesel::insert_into(schema::coin_infos::table)
            .values(items_to_insert)
            .on_conflict(coin_type_hash)
            .do_update()
            .set((
                transaction_version_created.eq(excluded(transaction_version_created)),
                creator_address.eq(excluded(creator_address)),
                name.eq(excluded(name)),
                symbol.eq(excluded(symbol)),
                decimals.eq(excluded(decimals)),
                transaction_created_timestamp.eq(excluded(transaction_created_timestamp)),
                supply_aggregator_table_handle.eq(excluded(supply_aggregator_table_handle)),
                supply_aggregator_table_key.eq(excluded(supply_aggregator_table_key)),
                inserted_at.eq(excluded(inserted_at)),
            )),
        Some(" WHERE coin_infos.transaction_version_created >= EXCLUDED.transaction_version_created "),
    )
}

fn insert_coin_balances_query(
    items_to_insert: Vec<CoinBalance>,
) -> (
    impl QueryFragment<Pg> + diesel::query_builder::QueryId + Send,
    Option<&'static str>,
) {
    use schema::coin_balances::dsl::*;

    (
        diesel::insert_into(schema::coin_balances::table)
            .values(items_to_insert)
            .on_conflict((transaction_version, owner_address, coin_type_hash))
            .do_nothing(),
        None,
    )
}

#[async_trait]
impl ProcessorTrait for FungibleAssetProcessor {
    fn name(&self) -> &'static str {
        ProcessorName::FungibleAssetProcessor.into()
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

        let processing_duration_in_secs = processing_start.elapsed().as_secs_f64();
        let db_insertion_start = std::time::Instant::now();

        let mut all_coin_activities = vec![];
        let mut all_coin_balances = vec![];
        let mut all_coin_infos: HashMap<String, CoinInfo> = HashMap::new();

        for txn in &transactions {
            let (mut coin_activities, mut coin_balances, coin_infos, ..) =
                CoinActivity::from_transaction(txn);
            all_coin_activities.append(&mut coin_activities);
            all_coin_balances.append(&mut coin_balances);
            // For coin infos, we only want to keep the first version, so insert only if key is not present already
            for (key, value) in coin_infos {
                all_coin_infos.entry(key).or_insert(value);
            }
        }
        let mut all_coin_infos = all_coin_infos.into_values().collect::<Vec<CoinInfo>>();
        all_coin_infos.sort_by(|a, b| a.coin_type.cmp(&b.coin_type));

        let tx_result = insert_to_db(
            self.get_pool(),
            self.name(),
            start_version,
            end_version,
            &all_coin_activities,
            &all_coin_infos,
            &all_coin_balances,
        )
        .await;
        let db_insertion_duration_in_secs = db_insertion_start.elapsed().as_secs_f64();
        match tx_result {
            Ok(_) => Ok(ProcessingResult::DefaultProcessingResult(
                DefaultProcessingResult {
                    start_version,
                    end_version,
                    processing_duration_in_secs,
                    db_insertion_duration_in_secs,
                    last_transaction_timestamp,
                },
            )),
            Err(err) => {
                error!(
                    start_version = start_version,
                    end_version = end_version,
                    processor_name = self.name(),
                    "[Parser] Error inserting transactions to db: {:?}",
                    err
                );
                bail!(format!("Error inserting transactions to db. Processor {}. Start {}. End {}. Error {:?}", self.name(), start_version, end_version, err))
            },
        }
    }

    fn connection_pool(&self) -> &ArcDbPool {
        &self.connection_pool
    }
}
