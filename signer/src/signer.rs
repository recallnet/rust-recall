// Copyright 2024 ADM Contributors
// SPDX-License-Identifier: Apache-2.0, MIT

use async_trait::async_trait;
use fendermint_crypto::SecretKey;
use fendermint_vm_actor_interface::eam::EthAddress;
use fendermint_vm_message::{chain::ChainMessage, signed::SignedMessage};
use fvm_ipld_encoding::RawBytes;
use fvm_shared::{
    address::Address, crypto::signature::Signature, econ::TokenAmount, message::Message, MethodNum,
};

use adm_provider::message::GasParams;
use adm_provider::util::get_delegated_address;

use crate::SubnetID;

/// Trait that must be implemented by all signers.
///
/// In the future, this could be implemented with WASM imports for browser-based wallets.
#[async_trait]
pub trait Signer: Clone + Send + Sync {
    /// Returns the signer address.
    fn address(&self) -> Address;

    /// Returns the signer EVM address.
    fn evm_address(&self) -> anyhow::Result<EthAddress> {
        let delegated = get_delegated_address(self.address())?;
        Ok(EthAddress::from(delegated))
    }

    /// Returns the signer [`SecretKey`] if it exists.
    fn secret_key(&self) -> Option<SecretKey>;

    /// Returns the signer [`SubnetID`] if it exists.
    ///
    /// This is used to derive a chain ID associated with a message.
    fn subnet_id(&self) -> Option<SubnetID>;

    /// Returns a [`ChainMessage`] that can be submitted to a provider.
    async fn transaction(
        &mut self,
        to: Address,
        value: TokenAmount,
        method_num: MethodNum,
        params: RawBytes,
        gas_params: GasParams,
    ) -> anyhow::Result<ChainMessage>;

    /// Returns a raw [`SignedMessage`].  
    fn sign_message(&self, message: Message) -> anyhow::Result<SignedMessage>;

    /// Verifies a raw [`SignedMessage`].
    fn verify_message(&self, message: &Message, signature: &Signature) -> anyhow::Result<()>;
}
