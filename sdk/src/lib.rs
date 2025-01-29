// Copyright 2025 Recall Contributors
// SPDX-License-Identifier: Apache-2.0, MIT

//! # Recall SDK
//!
//! The top-level user interface for managing Recall object storage and timehubs.

use recall_provider::message::GasParams;

pub mod account;
pub mod credits;
pub mod ipc;
pub mod machine;
pub mod network;
pub mod progress;
pub mod storage;
pub mod subnet;

/// Arguments common to transactions.
#[derive(Clone, Default, Debug)]
pub struct TxParams {
    /// Sender account sequence (nonce).
    pub sequence: Option<u64>,
    /// Gas params.
    pub gas_params: GasParams,
}
