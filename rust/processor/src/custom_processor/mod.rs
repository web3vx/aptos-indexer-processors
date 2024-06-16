// Copyright © Aptos Foundation
// SPDX-License-Identifier: Apache-2.0

use std::fmt::Debug;

use aptos_protos::transaction::v1::Transaction as ProtoTransaction;
use async_trait::async_trait;
use diesel::{ExpressionMethods, pg::upsert::excluded};
use enum_dispatch::enum_dispatch;
use serde::{Deserialize, Serialize};

use crate::{
    models::processor_status::ProcessorStatus,
    schema::processor_status,
    utils::{
        counters::{GOT_CONNECTION_COUNT, UNABLE_TO_GET_CONNECTION_COUNT},
        database::{execute_with_better_error, PgDbPool, PgPoolConnection},
        util::parse_timestamp,
    },
};
use crate::custom_processor::multisig_processor::MultisigProcessor;
use crate::processors::ProcessingResult;

pub mod multisig_processor;
mod utils;

/// Base trait for all processors
#[async_trait]
#[enum_dispatch]
pub trait CustomProcessorTrait: Send + Sync + Debug {
    fn name(&self) -> &'static str;

    /// Process all transactions including writing to the database
    async fn process_transactions(
        &self,
        transactions: Vec<ProtoTransaction>,
        start_version: u64,
        end_version: u64,
        db_chain_id: Option<u64>,
    ) -> anyhow::Result<()>;

    /// Gets a reference to the connection pool
    /// This is used by the `get_conn()` helper below
    fn connection_pool(&self) -> &PgDbPool;

    //* Below are helper methods that don't need to be implemented *//

    /// Gets an instance of the connection pool
    fn get_pool(&self) -> PgDbPool {
        let pool = self.connection_pool();
        pool.clone()
    }

    /// Gets the connection.
    /// If it was unable to do so (default timeout: 30s), it will keep retrying until it can.
    async fn get_conn(&self) -> PgPoolConnection {
        let pool = self.connection_pool();
        loop {
            match pool.get().await {
                Ok(conn) => {
                    GOT_CONNECTION_COUNT.inc();
                    return conn;
                },
                Err(err) => {
                    UNABLE_TO_GET_CONNECTION_COUNT.inc();
                    tracing::error!(
                        // todo bb8 doesn't let you read the connection timeout.
                        //"Could not get DB connection from pool, will retry in {:?}. Err: {:?}",
                        //pool.connection_timeout(),
                        "Could not get DB connection from pool, will retry. Err: {:?}",
                        err
                    );
                },
            };
        }
    }

    /// Store last processed version from database. We can assume that all previously processed
    /// versions are successful because any gap would cause the processor to panic
    async fn update_last_processed_version(
        &self,
        version: u64,
        last_transaction_timestamp: Option<aptos_protos::util::timestamp::Timestamp>,
    ) -> anyhow::Result<()> {
        let timestamp = last_transaction_timestamp.map(|t| parse_timestamp(&t, version as i64));
        let status = ProcessorStatus {
            processor: self.name().to_string(),
            last_success_version: version as i64,
            last_transaction_timestamp: timestamp,
        };
        execute_with_better_error(
            self.get_pool(),
            diesel::insert_into(processor_status::table)
                .values(&status)
                .on_conflict(processor_status::processor)
                .do_update()
                .set((
                    processor_status::last_success_version
                        .eq(excluded(processor_status::last_success_version)),
                    processor_status::last_updated.eq(excluded(processor_status::last_updated)),
                    processor_status::last_transaction_timestamp
                        .eq(excluded(processor_status::last_transaction_timestamp)),
                )),
            Some(" WHERE processor_status.last_success_version <= EXCLUDED.last_success_version "),
        )
        .await?;
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, strum::IntoStaticStr, strum::EnumDiscriminants)]
#[serde(tag = "type", rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
#[strum_discriminants(
    derive(
        Deserialize,
        Serialize,
        strum::EnumVariantNames,
        strum::IntoStaticStr,
        strum::Display,
        clap::ValueEnum
    ),
    name(CustomProcessorName),
    clap(rename_all = "snake_case"),
    serde(rename_all = "snake_case"),
    strum(serialize_all = "snake_case")
)]
pub enum CustomProcessorConfig {
    MultisigProcessor,
}
impl CustomProcessorConfig {
    pub fn name(&self) -> &'static str {
        self.into()
    }
}
/// This enum contains all the processors defined in this crate. We use enum_dispatch
/// as it is more efficient than using dynamic dispatch (Box<dyn ProcessorTrait>) and
/// it enables nice safety checks like in we do in `test_processor_names_complete`.

#[enum_dispatch(CustomProcessorTrait)]
#[derive(Debug)]
#[cfg_attr(
    test,
    derive(strum::EnumDiscriminants),
    strum_discriminants(
        derive(strum::EnumVariantNames),
        name(CustomProcessorDiscriminants),
        strum(serialize_all = "snake_case")
    )
)]
pub enum CustomProcessor {
    MultisigProcessor,
}

#[cfg(test)]
mod test {
    use strum::VariantNames;

    use super::*;

    /// This test exists to make sure that when a new processor is added, it is added
    /// to both Processor and ProcessorConfig. To make sure this passes, make sure the
    /// variants are in the same order (lexicographical) and the names match.
    #[test]
    fn test_processor_names_complete() {
        assert_eq!(
            CustomProcessorName::VARIANTS,
            CustomProcessorDiscriminants::VARIANTS
        );
    }
}