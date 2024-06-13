use crate::schema::multisig_transactions;
use crate::utils::database::PgDbPool;
use chrono::NaiveDateTime;
use diesel_async::RunQueryDsl;
use field_count::FieldCount;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub enum TransactionStatus {
    Pending = 1,
    Rejected = 2,
    Success = 3,
    Failed = 4,
}
#[derive(Clone, Debug, Deserialize, FieldCount, Identifiable, Insertable, Serialize)]
#[diesel(table_name = multisig_transactions)]
#[diesel(primary_key(transaction_id))]
pub struct MultisigTransaction {
    pub transaction_id: String,
    pub wallet_address: String,
    pub initiated_by: String,
    pub sequence_number: i64,
    pub payload: Value,
    pub status: i32,
    pub created_at: NaiveDateTime,
}
