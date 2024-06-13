use crate::schema::multisig_voting_transactions;
use chrono::NaiveDateTime;
use field_count::FieldCount;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, FieldCount, Insertable, Serialize)]
#[diesel(table_name = multisig_voting_transactions)]
pub struct MultisigVotingTransaction {
    pub wallet_address: String,
    pub owner_address: String,
    pub transaction_sequence: i64,
    pub value: bool,
    pub created_at: NaiveDateTime,
}
