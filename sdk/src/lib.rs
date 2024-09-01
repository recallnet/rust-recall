// Copyright 2024 Hoku Contributors
// SPDX-License-Identifier: Apache-2.0, MIT

//! # Hoku SDK
//!
//! The top-level user interface for managing Hoku object storage and state accumulators.

use hoku_provider::message::GasParams;

pub mod account;
pub mod ipc;
pub mod machine;
pub mod network;
pub mod progress;

/// Arguments common to transactions.
#[derive(Clone, Default, Debug)]
pub struct TxParams {
    /// Sender account sequence (nonce).
    pub sequence: Option<u64>,
    /// Gas params.
    pub gas_params: GasParams,
}
