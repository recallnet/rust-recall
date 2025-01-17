// Copyright 2024 Hoku Contributors
// SPDX-License-Identifier: Apache-2.0, MIT

use anyhow::anyhow;
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::signer::{EthAddress, Signer};
use crate::SubnetID;
use hoku_provider::tx::{BroadcastMode, DeliverTx, TxReceipt};
use hoku_provider::{
    fvm_ipld_encoding::RawBytes,
    fvm_shared::{address::Address, crypto::signature::Signature, econ::TokenAmount, MethodNum},
    message::{ChainMessage, GasParams, Message, OriginKind, SignedMessage},
    query::{FvmQueryHeight, QueryProvider},
    Client, Provider,
};

pub use fendermint_crypto::SecretKey;

/// Indicates how an [`Address`] should be derived from a public key.
///
/// [`AccountKind::Regular`] refers to native FVM addresses.
/// [`AccountKind::Ethereum`] refers to Ethereum style addresses.
#[derive(Debug, Clone)]
pub enum AccountKind {
    Regular,
    Ethereum,
}

/// [`Signer`] implementation that relies on a local [`SecretKey`] to sign messages.
///
/// Note, because [`Wallet`] manages the account's sequence (nonce) with a mutex,
/// using it across threads won't increase the speed at which it can sign messages.
#[derive(Debug, Clone)]
pub struct Wallet {
    addr: Address,
    sk: SecretKey,
    subnet_id: SubnetID,
    sequence: Arc<Mutex<u64>>,
}

#[async_trait]
impl Signer for Wallet {
    fn address(&self) -> Address {
        self.addr
    }

    fn secret_key(&self) -> Option<SecretKey> {
        Some(self.sk.clone())
    }

    fn subnet_id(&self) -> Option<SubnetID> {
        Some(self.subnet_id.clone())
    }

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
        mut gas_params: GasParams,
        broadcast_mode: BroadcastMode,
        decode_fn: F,
    ) -> anyhow::Result<TxReceipt<T>> {
        let mut message = Message {
            version: Default::default(),
            from: self.addr,
            to,
            sequence: 0,
            value,
            method_num,
            params,
            gas_limit: gas_params.gas_limit.clone(),
            gas_fee_cap: gas_params.gas_fee_cap.clone(),
            gas_premium: gas_params.gas_premium.clone(),
        };
        // Set gas limit to the estimated value
        let gas_limit = provider
            .estimate_gas_limit(message.clone(), FvmQueryHeight::Committed)
            .await?;
        message.gas_limit = gas_limit;

        // Set sequence to the current value
        let mut sequence_guard = self.sequence.lock().await;
        let sequence = *sequence_guard;
        message.sequence = sequence;

        // Set gas params to the estimated value
        gas_params.set_limits();

        *sequence_guard += 1;
        let signed = SignedMessage::new_secp256k1(message, &self.sk, &self.subnet_id.chain_id())?;
        let signed_message = ChainMessage::Signed(signed);
        Ok(provider
            .perform(signed_message, broadcast_mode, decode_fn)
            .await?)
    }

    fn sign_message(&self, message: Message) -> anyhow::Result<SignedMessage> {
        let signed = SignedMessage::new_secp256k1(message, &self.sk, &self.subnet_id.chain_id())?;
        Ok(signed)
    }

    fn verify_message(&self, message: &Message, signature: &Signature) -> anyhow::Result<()> {
        SignedMessage::verify_signature(
            OriginKind::Fvm,
            message,
            signature,
            &self.subnet_id.chain_id(),
        )?;
        Ok(())
    }
}

impl Wallet {
    /// Returns a new secp256k1 [`Wallet`] using the given [`SecretKey`].
    ///
    /// Note, subnets only support [`AccountKind::Ethereum`].
    pub fn new_secp256k1(
        sk: SecretKey,
        kind: AccountKind,
        subnet_id: SubnetID,
    ) -> anyhow::Result<Self> {
        let pk = sk.public_key().serialize();
        let addr = match kind {
            AccountKind::Regular => Address::new_secp256k1(&pk)?,
            AccountKind::Ethereum => Address::from(EthAddress::new_secp256k1(&pk)?),
        };
        let sequence = Arc::new(Mutex::new(0));
        Ok(Wallet {
            sk,
            addr,
            subnet_id,
            sequence,
        })
    }

    /// Inititalize sequence from the actor's on-chain state.
    pub async fn init_sequence(&mut self, provider: &impl QueryProvider) -> anyhow::Result<()> {
        // Using the `Pending` state to query just in case there are other transactions initiated by the signer.
        let res = provider
            .actor_state(&self.addr, FvmQueryHeight::Pending)
            .await?;

        match res.value {
            Some((_, state)) => {
                let mut sequence_guard = self.sequence.lock().await;
                *sequence_guard = state.sequence;
                Ok(())
            }
            None => Err(anyhow!(
                "failed to init sequence; actor {} cannot be found",
                self.addr
            )),
        }
    }

    /// Set the sequence to the given value.
    /// If `maybe_sequence` is `None`, it's fetched from the actor's on-chain state.
    pub async fn set_sequence(
        &mut self,
        maybe_sequence: Option<u64>,
        provider: &impl QueryProvider,
    ) -> anyhow::Result<()> {
        if let Some(sequence) = maybe_sequence {
            let mut sequence_guard = self.sequence.lock().await;
            *sequence_guard = sequence;
        } else {
            self.init_sequence(provider).await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;
    use async_trait::async_trait;
    use hoku_provider::query::{FvmQuery, FvmQueryHeight};
    use tendermint_rpc::endpoint::abci_query::AbciQuery;

    struct MockQueryProvider;

    #[async_trait]
    impl QueryProvider for MockQueryProvider {
        async fn query(
            &self,
            _query: FvmQuery,
            _height: FvmQueryHeight,
        ) -> anyhow::Result<AbciQuery> {
            // mocked query response with a sequence == 65
            let response = r#"{
                "code": 0,
                "log": "",
                "info": "",
                "index": "0",
                "key": "GIM=",
                "value": "pWRjb2Rl2CpYJwABVaDkAiBuyxJuuG2pHLjnd5bBzV8iF37KrDAjVytY6h9tvJ3WT2VzdGF0ZdgqWCcAAXGg5AIgRbDPwiDO7Ft8HGLE1Bk9OOTrpI6IFXKc51+cCrDkwcBoc2VxdWVuY2UYQWdiYWxhbmNlSQB84tz/LZSh2HFkZWxlZ2F0ZWRfYWRkcmVzc1YECsBf5rY/+ks8UY5v8eWXNY7oOdsB",
                "proof": null,
                "height": "580876",
                "codespace": ""
              }"#;
            let parsed: AbciQuery = serde_json::from_str(response)?;
            Ok(parsed)
        }
    }

    #[tokio::test]
    async fn test_set_sequence() {
        let mock_provider = MockQueryProvider;
        let private_key = crate::key::random_secretkey();
        let subnet_id = SubnetID::from_str("r/foobar").unwrap();
        let mut wallet =
            Wallet::new_secp256k1(private_key.clone(), AccountKind::Ethereum, subnet_id).unwrap();

        // Test setting a specific sequence value
        wallet.set_sequence(Some(50), &mock_provider).await.unwrap();
        assert_eq!(*wallet.sequence.lock().await, 50);

        // Test initializing sequence from provider
        wallet.set_sequence(None, &mock_provider).await.unwrap();
        assert_eq!(*wallet.sequence.lock().await, 65);
    }
}
