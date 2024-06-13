use crate::schema::multisig_owners;
use chrono::NaiveDateTime;
use field_count::FieldCount;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Deserialize, FieldCount, Identifiable, Insertable, Serialize)]
#[diesel(primary_key(owner_address))]
#[diesel(table_name = multisig_owners)]
pub struct MultisigOwner {
    pub owner_address: String,
    pub created_at: NaiveDateTime,
}
