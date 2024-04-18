// Copyright 2024 ADM Contributors
// SPDX-License-Identifier: Apache-2.0, MIT

use anyhow::anyhow;
use fendermint_crypto::SecretKey;
use fendermint_vm_actor_interface::eam::EthAddress;
use fendermint_vm_message::{
    chain::ChainMessage, query::FvmQueryHeight, signed::Object, signed::SignedMessage,
};
use fvm_ipld_encoding::RawBytes;
use fvm_shared::{
    address::Address, chainid::ChainID, econ::TokenAmount, message::Message, MethodNum,
};

use adm_provider::{message::GasParams, QueryProvider};

use crate::signer::Signer;

#[derive(Debug, Clone)]
pub enum AccountKind {
    Regular,
    Ethereum,
}

#[derive(Debug, Clone)]
pub struct Wallet {
    sk: SecretKey,
    addr: Address,
    chain_id: ChainID,
    sequence: u64, // TODO: make send + sync
}

impl Signer for Wallet {
    fn address(&self) -> Address {
        self.addr
    }

    fn transaction(
        &mut self,
        to: Address,
        value: TokenAmount,
        method_num: MethodNum,
        params: RawBytes,
        object: Option<Object>,
        gas_params: GasParams,
    ) -> anyhow::Result<ChainMessage> {
        let message = Message {
            version: Default::default(),
            from: self.addr,
            to,
            sequence: self.sequence,
            value,
            method_num,
            params,
            gas_limit: gas_params.gas_limit,
            gas_fee_cap: gas_params.gas_fee_cap,
            gas_premium: gas_params.gas_premium,
        };
        self.sequence += 1;
        let signed = SignedMessage::new_secp256k1(message, object, &self.sk, &self.chain_id)?;
        Ok(ChainMessage::Signed(signed))
    }

    // TODO: Add stateless sign+verify methods.
}

impl Wallet {
    pub fn new_secp256k1(
        sk: SecretKey,
        kind: AccountKind,
        chain_id: ChainID,
    ) -> anyhow::Result<Self> {
        let pk = sk.public_key().serialize();
        let addr = match kind {
            AccountKind::Regular => Address::new_secp256k1(&pk)?,
            AccountKind::Ethereum => Address::from(EthAddress::new_secp256k1(&pk)?),
        };
        Ok(Wallet {
            sk,
            addr,
            chain_id,
            sequence: 0,
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
                self.sequence = state.sequence;
                Ok(())
            }
            None => Err(anyhow!(
                "failed to init sequence; actor {} cannot be found",
                self.addr
            )),
        }
    }

    /// Set the sequence to an arbitrary value.
    pub fn set_sequence(&mut self, sequence: u64) {
        self.sequence = sequence;
    }
}
