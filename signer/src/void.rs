// Copyright 2024 Hoku Contributors
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::signer::Signer;
use crate::SubnetID;
use anyhow::anyhow;
use async_trait::async_trait;
use fendermint_crypto::SecretKey;
use hoku_provider::fvm_ipld_encoding::RawBytes;
use hoku_provider::fvm_shared::{
    address::Address, crypto::signature::Signature, econ::TokenAmount, message::Message, MethodNum,
};
use hoku_provider::message::{GasParams, SignedMessage};
use hoku_provider::tx::{BroadcastMode, DeliverTx, TxReceipt};
use hoku_provider::{Client, Provider};

/// [`Signer`] implementation that is not capable of signing messages.
#[derive(Clone, Debug)]
pub struct Void {
    address: Address,
}

impl Void {
    pub fn new(address: Address) -> Self {
        Self { address }
    }
}

#[async_trait]
impl Signer for Void {
    fn address(&self) -> Address {
        self.address
    }

    fn secret_key(&self) -> Option<SecretKey> {
        None
    }

    fn subnet_id(&self) -> Option<SubnetID> {
        None
    }

    async fn send_transaction<
        C: Client + Send + Sync,
        T: Send + Sync,
        F: FnOnce(&DeliverTx) -> anyhow::Result<T> + Send + Sync,
    >(
        &mut self,
        _provider: &impl Provider<C>,
        _to: Address,
        _value: TokenAmount,
        _method_num: MethodNum,
        _params: RawBytes,
        _gas_params: GasParams,
        _broadcast_mode: BroadcastMode,
        _decode_fn: F,
    ) -> anyhow::Result<TxReceipt<T>> {
        Err(anyhow!("void signer cannot create transactions"))
    }

    fn sign_message(&self, _message: Message) -> anyhow::Result<SignedMessage> {
        Err(anyhow!("void signer cannot sign messages"))
    }

    fn verify_message(&self, _message: &Message, _signature: &Signature) -> anyhow::Result<()> {
        Err(anyhow!("void signer cannot verify messages"))
    }
}
