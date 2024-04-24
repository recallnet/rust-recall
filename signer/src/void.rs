// Copyright 2024 ADM Contributors
// SPDX-License-Identifier: Apache-2.0, MIT

use anyhow::anyhow;
use fendermint_vm_message::{chain::ChainMessage, signed::Object, signed::SignedMessage};
use fvm_ipld_encoding::RawBytes;
use fvm_shared::{address::Address, econ::TokenAmount, message::Message, MethodNum};

use adm_provider::message::GasParams;

use crate::signer::Signer;

#[derive(Clone)]
pub struct Void {}

impl Signer for Void {
    fn address(&self) -> Address {
        Address::new_id(0)
    }

    fn transaction(
        &mut self,
        _to: Address,
        _value: TokenAmount,
        _method_num: MethodNum,
        _params: RawBytes,
        _object: Option<Object>,
        _gas_params: GasParams,
    ) -> anyhow::Result<ChainMessage> {
        Err(anyhow!("void signer cannot create transactions"))
    }

    fn sign_message(
        &self,
        _message: Message,
        _object: Option<Object>,
    ) -> anyhow::Result<SignedMessage> {
        Err(anyhow!("void signer cannot sign messages"))
    }
}
