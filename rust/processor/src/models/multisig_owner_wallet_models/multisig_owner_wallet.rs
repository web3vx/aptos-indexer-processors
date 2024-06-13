use crate::schema::owners_wallets;
use chrono::NaiveDateTime;
use field_count::FieldCount;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, FieldCount, Identifiable, Insertable, Serialize)]
#[diesel(table_name = owners_wallets)]
#[diesel(primary_key(owner_address, wallet_address))]
pub struct OwnersWallet {
    pub owner_address: String,
    pub wallet_address: String,
    pub created_at: NaiveDateTime,
}
