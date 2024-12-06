// Copyright 2024 Hoku Contributors
// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

use fendermint_vm_actor_interface::system::SYSTEM_ACTOR_ADDR;
use fendermint_vm_message::chain::ChainMessage;
use fendermint_vm_message::signed::SignedMessage;
use fvm_ipld_encoding::RawBytes;
use fvm_shared::{address::Address, econ::TokenAmount, message::Message, MethodNum};

const MIN_GAS_FEE_CAP: u64 = 100;
const MIN_GAS_PREMIUM: u64 = 100_000;

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
            gas_limit: fvm_shared::BLOCK_GAS_LIMIT,
            gas_fee_cap: TokenAmount::from_atto(MIN_GAS_FEE_CAP),
            gas_premium: TokenAmount::from_atto(MIN_GAS_PREMIUM),
        }
    }
}

impl GasParams {
    /// Sets limits on the gas params.
    ///
    /// Note: Currently a user could set gas_fee_cap and/or gas_premium to zero.
    /// https://github.com/consensus-shipyard/ipc/pull/1185 fixes this.
    /// In the meantime, we enforce limits in the client.
    pub fn set_limits(&mut self) {
        if self.gas_limit == 0 || self.gas_limit > fvm_shared::BLOCK_GAS_LIMIT {
            self.gas_limit = fvm_shared::BLOCK_GAS_LIMIT;
        }
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

/// Convenience method to create a local unsigned read-only object-carrying message.
pub fn object_upload_message(
    from: Address,
    to: Address,
    method_num: MethodNum,
    params: RawBytes,
) -> Message {
    Message {
        version: Default::default(),
        from,
        to,
        sequence: 0,
        value: Default::default(),
        method_num,
        params,
        gas_limit: Default::default(),
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
