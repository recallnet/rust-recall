// Copyright 2024 Hoku Contributors
// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

use std::str::FromStr;

use anyhow::anyhow;
use async_trait::async_trait;
use fendermint_vm_message::chain::ChainMessage;
use num_traits::Zero;
use serde::Serialize;
use tendermint::{abci::response::DeliverTx, block::Height, Hash};

/// Controls how the provider waits for the result of a transaction.
#[derive(Debug, Default, Copy, Clone)]
pub enum BroadcastMode {
    /// Return immediately after the transaction is broadcasted without waiting for check results.
    Async,
    /// Wait for the check results before returning from broadcast.
    Sync,
    /// Wait for the delivery results before returning from broadcast.
    #[default]
    Commit,
}

impl FromStr for BroadcastMode {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "async" => Self::Async,
            "sync" => Self::Sync,
            "commit" => Self::Commit,
            _ => return Err(anyhow!("invalid broadcast mode")),
        })
    }
}

/// The current status of a transaction.
#[derive(Debug, Copy, Clone, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TxStatus {
    /// The transaction is in the memory pool waiting to be included in a block.
    Pending,
    /// The transaction has been committed to a finalized block.
    Committed,
}

/// The receipt of a transaction.
#[derive(Debug, Copy, Clone, Serialize)]
pub struct TxReceipt<T>
where
    T: 'static,
{
    /// The transaction's current status.
    pub status: TxStatus,
    /// The hash of the transaction.
    pub hash: Hash,
    /// The block height at which the transaction was included.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<Height>,
    /// Gas used by the transaction.
    #[serde(skip_serializing_if = "i64::is_zero")]
    pub gas_used: i64,
    /// Data returned by the transaction.
    #[serde(skip_serializing_if = "is_data_empty")]
    pub data: Option<T>,
}

fn is_data_empty<T>(data: &Option<T>) -> bool
where
    T: 'static,
{
    match data {
        None => true,
        Some(_) if std::any::TypeId::of::<T>() == std::any::TypeId::of::<()>() => true,
        _ => false,
    }
}

impl<D> TxReceipt<D> {
    /// Create a new receipt with status pending.
    pub fn pending(hash: Hash) -> Self {
        TxReceipt {
            status: TxStatus::Pending,
            hash,
            height: None,
            gas_used: 0,
            data: None,
        }
    }

    /// Create a new receipt with status committed.
    pub fn committed(hash: Hash, height: Height, gas_used: i64, data: Option<D>) -> Self {
        TxReceipt {
            status: TxStatus::Committed,
            hash,
            height: Some(height),
            gas_used,
            data,
        }
    }
}

/// Provider for submitting transactions.
#[async_trait]
pub trait TxProvider: Send + Sync {
    /// Perform the sending of a chain message.
    async fn perform<F, T>(
        &self,
        message: ChainMessage,
        broadcast_mode: BroadcastMode,
        f: F,
    ) -> anyhow::Result<TxReceipt<T>>
    where
        F: FnOnce(&DeliverTx) -> anyhow::Result<T> + Sync + Send,
        T: Sync + Send;
}
