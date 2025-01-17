// Copyright 2024 Hoku Contributors
// SPDX-License-Identifier: Apache-2.0, MIT

use async_trait::async_trait;
use hoku_provider::message::{GasParams, Message, SignedMessage};
use hoku_provider::tx::{BroadcastMode, DeliverTx, TxReceipt};
use hoku_provider::util::get_eth_address;
use hoku_provider::{
    fvm_ipld_encoding::RawBytes,
    fvm_shared::{address::Address, crypto::signature::Signature, econ::TokenAmount, MethodNum},
    Client, Provider,
};

use crate::key::SecretKey;
use crate::SubnetID;

pub use fendermint_vm_actor_interface::eam::EthAddress;

/// Trait that must be implemented by all signers.
///
/// In the future, this could be implemented with WASM imports for browser-based wallets.
#[async_trait]
pub trait Signer: Clone + Send + Sync {
    /// Returns the signer address.
    fn address(&self) -> Address;

    /// Returns the signer Ethereum address.
    fn eth_address(&self) -> anyhow::Result<EthAddress> {
        let delegated = get_eth_address(self.address())?;
        Ok(EthAddress::from(delegated))
    }

    /// Returns the signer [`SecretKey`] if it exists.
    fn secret_key(&self) -> Option<SecretKey>;

    /// Returns the signer [`SubnetID`] if it exists.
    ///
    /// This is used to derive a chain ID associated with a message.
    fn subnet_id(&self) -> Option<SubnetID>;

    /// Returns a [`ChainMessage`] that can be submitted to a provider.
    async fn send_transaction<
        C: Client + Send + Sync,
        T: Send + Sync,
        F: FnOnce(&DeliverTx) -> anyhow::Result<T> + Send + Sync,
    >(
        &mut self,
        provider: &impl Provider<C>,
        to: Address,
        value: TokenAmount,
        method_num: MethodNum,
        params: RawBytes,
        gas_params: GasParams,
        broadcast_mode: BroadcastMode,
        decode_fn: F,
    ) -> anyhow::Result<TxReceipt<T>>;

    /// Returns a raw [`SignedMessage`].  
    fn sign_message(&self, message: Message) -> anyhow::Result<SignedMessage>;

    /// Verifies a raw [`SignedMessage`].
    fn verify_message(&self, message: &Message, signature: &Signature) -> anyhow::Result<()>;
}
