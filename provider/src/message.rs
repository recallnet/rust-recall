// Copyright 2025 Recall Contributors
// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

use fvm_shared::{address::Address, econ::TokenAmount};
use recall_fendermint_vm_actor_interface::system::SYSTEM_ACTOR_ADDR;

pub use crate::{
    fvm_ipld_encoding::RawBytes,
    fvm_shared::{message::Message, MethodNum},
};
pub use recall_fendermint_vm_message::{
    chain::ChainMessage,
    signed::{OriginKind, SignedMessage},
};

const MIN_GAS_FEE_CAP: u64 = 100;
const MIN_GAS_PREMIUM: u64 = 1;

/// Gas parameters for transactions.
#[derive(Clone, Debug)]
pub struct GasParams {
    /// Maximum amount of gas that can be charged.
    pub gas_limit: u64,
    /// Price of gas.
    ///
    /// Any discrepancy between this and the base fee is paid for
    /// by the validator who puts the transaction into the block.
    /// The client will enforce a minimum value of 100 attoFIL.
    pub gas_fee_cap: TokenAmount,
    /// Gas premium.
    ///
    /// The client will enforce a minimum value of 100,000 attoFIL.
    pub gas_premium: TokenAmount,
}

impl Default for GasParams {
    fn default() -> Self {
        GasParams {
            gas_limit: 0,
            gas_fee_cap: TokenAmount::from_atto(MIN_GAS_FEE_CAP),
            gas_premium: TokenAmount::from_atto(MIN_GAS_PREMIUM),
        }
    }
}

impl GasParams {
    /// Sets limits on the gas params.
    ///
    /// Note: Currently a user could set gas_fee_cap to zero.
    /// See <https://github.com/consensus-shipyard/ipc/pull/1185#issuecomment-2549333793>.
    /// In the meantime, we enforce limits in the client.
    pub fn set_limits(&mut self) {
        let min_gas_fee_cap = TokenAmount::from_atto(MIN_GAS_FEE_CAP);
        if self.gas_fee_cap < min_gas_fee_cap {
            self.gas_fee_cap = min_gas_fee_cap;
        }
        let min_gas_premium = TokenAmount::from_atto(MIN_GAS_PREMIUM);
        if self.gas_premium < min_gas_premium {
            self.gas_premium = min_gas_premium;
        }
    }
}

/// Convenience method to create a local unsigned read-only message.
pub fn local_message(to: Address, method_num: MethodNum, params: RawBytes) -> Message {
    Message {
        version: Default::default(),
        from: SYSTEM_ACTOR_ADDR,
        to,
        sequence: 0,
        value: Default::default(),
        method_num,
        params,
        gas_limit: fvm_shared::BLOCK_GAS_LIMIT,
        gas_fee_cap: Default::default(),
        gas_premium: Default::default(),
    }
}

/// Convenience method to serialize a [`ChainMessage`] for inclusion in a Tendermint transaction.
pub fn serialize(message: &ChainMessage) -> anyhow::Result<Vec<u8>> {
    Ok(fvm_ipld_encoding::to_vec(message)?)
}

/// Convenience method to serialize a [`SignedMessage`] for authentication.
pub fn serialize_signed(message: &SignedMessage) -> anyhow::Result<Vec<u8>> {
    Ok(fvm_ipld_encoding::to_vec(message)?)
}
