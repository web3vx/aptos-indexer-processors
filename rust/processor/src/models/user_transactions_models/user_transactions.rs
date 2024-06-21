// Copyright © Aptos Foundation

// Copyright (c) Aptos
// SPDX-License-Identifier: Apache-2.0

// This is required because a diesel macro makes clippy sad
#![allow(clippy::extra_unused_lifetimes)]
#![allow(clippy::unused_unit)]

use super::signatures::Signature;
use crate::{
    schema::user_transactions,
    utils::util::{
        get_entry_function_from_user_request, parse_timestamp, standardize_address,
        u64_to_bigdecimal,
    },
};
use aptos_protos::{
    transaction::v1::{UserTransaction as UserTransactionPB, UserTransactionRequest},
    util::timestamp::Timestamp,
};
use field_count::FieldCount;
use serde::{Deserialize, Serialize};

#[derive(Clone, Deserialize, Debug, FieldCount, Identifiable, Insertable, Serialize)]
#[diesel(primary_key(version))]
#[diesel(table_name = user_transactions)]
pub struct UserTransaction {
    pub version: i64,
    pub sender: String,
    pub entry_function_id_str: String,
}


#[derive(Clone, Deserialize, Debug, FieldCount, Identifiable, Insertable, Serialize)]
#[diesel(primary_key(version))]
#[diesel(table_name = user_transactions)]
pub struct UserTransactionModelWithoutEntryFunctionIdStr {
    pub version: i64,
    pub sender: String,
}

impl UserTransaction {
    pub fn from_transaction(
        txn: &UserTransactionPB,
        timestamp: &Timestamp,
        block_height: i64,
        epoch: i64,
        version: i64,
    ) -> (Self, Vec<Signature>) {
        let user_request = txn
            .request
            .as_ref()
            .expect("Sends is not present in user txn");
        (
            Self {
                version,
                sender: standardize_address(&user_request.sender),
                entry_function_id_str: get_entry_function_from_user_request(user_request)
                    .unwrap_or_default(),
            },
            Self::get_signatures(user_request, version, block_height),
        )
    }

    /// Empty vec if signature is None
    pub fn get_signatures(
        user_request: &UserTransactionRequest,
        version: i64,
        block_height: i64,
    ) -> Vec<Signature> {
        user_request
            .signature
            .as_ref()
            .map(|s| {
                Signature::from_user_transaction(s, &user_request.sender, version, block_height)
                    .unwrap()
            })
            .unwrap_or_default()
    }
}

// Prevent conflicts with other things named `Transaction`
pub type UserTransactionModel = UserTransaction;
