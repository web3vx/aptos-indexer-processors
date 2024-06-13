use crate::schema::multisig_wallets;
use chrono::NaiveDateTime;
use field_count::FieldCount;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Deserialize, FieldCount, Identifiable, Insertable, Serialize)]
#[diesel(primary_key(wallet_address))]
#[diesel(table_name = multisig_wallets)]
pub struct MultisigWallet {
    pub wallet_address: String,
    pub required_signatures: i32,
    pub metadata: Option<serde_json::Value>,
    pub created_at: NaiveDateTime,
}
