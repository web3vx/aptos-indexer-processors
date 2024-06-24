use chrono::NaiveDateTime;
use field_count::FieldCount;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::schema::multisig_transactions;

pub enum TransactionStatus {
    Pending = 1,
    Rejected = 2,
    Success = 3,
    Failed = 4,
}

#[derive(
    Clone,
    Debug,
    Deserialize,
    FieldCount,
    Identifiable,
    Queryable,
    Insertable,
    Serialize,
    AsChangeset,
)]
#[diesel(table_name = multisig_transactions)]
#[diesel(primary_key(wallet_address, sequence_number))]
pub struct MultisigTransaction {
    pub wallet_address: String,
    pub initiated_by: String,
    pub sequence_number: i32,
    pub payload: Value,
    pub payload_hash: Option<Value>,
    pub status: i32,
    pub created_at: NaiveDateTime,
    pub executed_at: Option<NaiveDateTime>,
    pub executor: Option<String>,
}
