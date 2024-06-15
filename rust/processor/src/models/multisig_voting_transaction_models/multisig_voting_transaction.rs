use crate::schema::multisig_voting_transactions;
use chrono::NaiveDateTime;
use field_count::FieldCount;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, FieldCount, Insertable, Serialize)]
#[diesel(table_name = multisig_voting_transactions)]
#[diesel(primary_key(transaction_sequence, wallet_address, value))]
pub struct MultisigVotingTransaction {
    pub wallet_address: String,
    pub transaction_sequence: i32,
    pub value: bool,
    pub created_at: NaiveDateTime,
}
