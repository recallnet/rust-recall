// Copyright 2025 Recall Contributors
// SPDX-License-Identifier: Apache-2.0, MIT

use anyhow::anyhow;
use fendermint_actor_blobs_shared::{
    accounts::{Account, GetAccountParams},
    method::Method::{GetAccount, GetStats},
    GetStatsReturn,
};
use fendermint_vm_actor_interface::blobs::BLOBS_ACTOR_ADDR;
use recall_provider::{
    fvm_ipld_encoding,
    fvm_shared::address::Address,
    message::{local_message, RawBytes},
    query::{FvmQueryHeight, QueryProvider},
    response::decode_bytes,
};
use serde::{Deserialize, Serialize};
use tendermint::abci::response::DeliverTx;

/// Storage usage stats for an account.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Usage {
    // Total size of all blobs managed by the account.
    pub capacity_used: String,
}

impl Default for Usage {
    fn default() -> Self {
        Self {
            capacity_used: "0".into(),
        }
    }
}

impl From<Account> for Usage {
    fn from(v: Account) -> Self {
        Self {
            capacity_used: v.capacity_used.to_string(),
        }
    }
}

/// Subnet-wide storage statistics.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StorageStats {
    /// The total free storage capacity of the subnet.
    pub capacity_free: String,
    /// The total used storage capacity of the subnet.
    pub capacity_used: String,
    /// Total number of actively stored blobs.
    pub num_blobs: u64,
    /// Total number of currently resolving blobs.
    pub num_resolving: u64,
    /// Total number of debit accounts.
    pub num_accounts: u64,
    /// Total bytes of all currently resolving blobs.
    pub bytes_resolving: u64,
    /// Total number of blobs that are not yet added to the validator's resolve pool.
    pub num_added: u64,
    // Total bytes of all blobs that are not yet added to the validator's resolve pool.
    pub bytes_added: u64,
}

impl From<GetStatsReturn> for StorageStats {
    fn from(v: GetStatsReturn) -> Self {
        Self {
            capacity_free: v.capacity_free.to_string(),
            capacity_used: v.capacity_used.to_string(),
            num_blobs: v.num_blobs,
            num_resolving: v.num_resolving,
            num_accounts: v.num_accounts,
            bytes_resolving: v.bytes_resolving,
            num_added: v.num_added,
            bytes_added: v.bytes_added,
        }
    }
}

/// A static wrapper around Recall storage methods.
pub struct Storage {}

impl Storage {
    pub async fn stats(
        provider: &impl QueryProvider,
        height: FvmQueryHeight,
    ) -> anyhow::Result<StorageStats> {
        let message = local_message(BLOBS_ACTOR_ADDR, GetStats as u64, Default::default());
        let response = provider.call(message, height, decode_stats).await?;
        Ok(response.value)
    }

    pub async fn usage(
        provider: &impl QueryProvider,
        address: Address,
        height: FvmQueryHeight,
    ) -> anyhow::Result<Usage> {
        let params = GetAccountParams(address);
        let params = RawBytes::serialize(params)?;
        let message = local_message(BLOBS_ACTOR_ADDR, GetAccount as u64, params);
        let response = provider.call(message, height, decode_usage).await?;
        if let Some(account) = response.value {
            Ok(account)
        } else {
            Ok(Usage::default())
        }
    }
}

fn decode_stats(deliver_tx: &DeliverTx) -> anyhow::Result<StorageStats> {
    let data = decode_bytes(deliver_tx)?;
    fvm_ipld_encoding::from_slice::<GetStatsReturn>(&data)
        .map(|v| v.into())
        .map_err(|e| anyhow!("error parsing as storage stats: {e}"))
}

fn decode_usage(deliver_tx: &DeliverTx) -> anyhow::Result<Option<Usage>> {
    let data = decode_bytes(deliver_tx)?;
    fvm_ipld_encoding::from_slice::<Option<Account>>(&data)
        .map(|v| v.map(|v| v.into()))
        .map_err(|e| anyhow!("error parsing as storage usage: {e}"))
}
