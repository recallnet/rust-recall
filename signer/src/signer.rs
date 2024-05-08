// Copyright 2024 ADM Contributors
// SPDX-License-Identifier: Apache-2.0, MIT

use async_trait::async_trait;
use fendermint_crypto::SecretKey;
use fendermint_vm_message::{chain::ChainMessage, signed::Object, signed::SignedMessage};
use fvm_ipld_encoding::RawBytes;
use fvm_shared::{
    address::Address, crypto::signature::Signature, econ::TokenAmount, message::Message, MethodNum,
};

use adm_provider::message::GasParams;

use crate::SubnetID;

#[async_trait]
pub trait Signer: Clone + Send + Sync {
    fn address(&self) -> Address;

    fn secret_key(&self) -> Option<SecretKey>;

    fn subnet_id(&self) -> Option<SubnetID>;

    async fn transaction(
        &mut self,
        to: Address,
        value: TokenAmount,
        method_num: MethodNum,
        params: RawBytes,
        object: Option<Object>,
        gas_params: GasParams,
    ) -> anyhow::Result<ChainMessage>;

    fn sign_message(
        &self,
        message: Message,
        object: Option<Object>,
    ) -> anyhow::Result<SignedMessage>;

    fn verify_message(
        &self,
        message: &Message,
        object: &Option<Object>,
        signature: &Signature,
    ) -> anyhow::Result<()>;
}
