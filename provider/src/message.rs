// Copyright 2024 ADM Contributors
// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

use fendermint_vm_actor_interface::system::SYSTEM_ACTOR_ADDR;
use fendermint_vm_message::chain::ChainMessage;
use fendermint_vm_message::signed::SignedMessage;
use fvm_ipld_encoding::RawBytes;
use fvm_shared::{address::Address, econ::TokenAmount, message::Message, MethodNum};

#[derive(Clone, Debug)]
pub struct GasParams {
    /// Maximum amount of gas that can be charged.
    pub gas_limit: u64,
    /// Price of gas.
    ///
    /// Any discrepancy between this and the base fee is paid for
    /// by the validator who puts the transaction into the block.
    pub gas_fee_cap: TokenAmount,
    /// Gas premium.
    pub gas_premium: TokenAmount,
}

impl Default for GasParams {
    fn default() -> Self {
        GasParams {
            gas_limit: fvm_shared::BLOCK_GAS_LIMIT,
            gas_fee_cap: Default::default(),
            gas_premium: Default::default(),
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
        gas_limit: Default::default(),
        gas_fee_cap: Default::default(),
        gas_premium: Default::default(),
    }
}

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
