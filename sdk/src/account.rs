// Copyright 2024 Hoku Contributors
// SPDX-License-Identifier: Apache-2.0, MIT

use anyhow::anyhow;
use fendermint_actor_blobs_shared::params::SetSponsorParams;
use fendermint_actor_blobs_shared::Method::SetAccountSponsor;
use fendermint_vm_actor_interface::blobs::BLOBS_ACTOR_ADDR;

use hoku_provider::fvm_ipld_encoding::RawBytes;
use hoku_provider::message::GasParams;
use hoku_provider::response::decode_empty;
use hoku_provider::tx::{BroadcastMode, TxReceipt};
use hoku_provider::{
    fvm_shared::{address::Address, econ::TokenAmount},
    query::{FvmQueryHeight, QueryProvider},
    Client, Provider,
};
use hoku_signer::Signer;

pub use crate::ipc::{manager::EvmManager, subnet::EVMSubnet};
pub use ethers::prelude::TransactionReceipt;

/// Options for setting credit sponsor.
#[derive(Clone, Default, Debug)]
pub struct SetSponsorOptions {
    /// Broadcast mode for the transaction.
    pub broadcast_mode: BroadcastMode,
    /// Gas params for the transaction.
    pub gas_params: GasParams,
}

/// A static wrapper around Hoku account methods.
pub struct Account {}

impl Account {
    /// Get the sequence (nonce) for a [`Signer`] at the given height.
    pub async fn sequence(
        provider: &impl QueryProvider,
        signer: &impl Signer,
        height: FvmQueryHeight,
    ) -> anyhow::Result<u64> {
        let response = provider.actor_state(&signer.address(), height).await?;

        match response.value {
            Some((_, state)) => Ok(state.sequence),
            None => Err(anyhow!(
                "failed to get sequence; actor {} cannot be found",
                signer.address()
            )),
        }
    }

    /// Get the balance for a [`Signer`] at the given height.
    pub async fn balance(signer: &impl Signer, subnet: EVMSubnet) -> anyhow::Result<TokenAmount> {
        EvmManager::balance(signer.address(), subnet).await
    }

    /// Deposit funds from a [`Signer`] to an address in the given subnet.
    pub async fn deposit(
        signer: &impl Signer,
        to: Address,
        subnet: EVMSubnet,
        amount: TokenAmount,
    ) -> anyhow::Result<TransactionReceipt> {
        // Approve the gateway to spend funds on behalf of the user.
        // This is required when the subnet uses a custom ERC20 token as
        // the gateway's supply source.
        EvmManager::approve_gateway(signer, subnet.clone(), amount.clone()).await?;
        EvmManager::deposit(signer, to, subnet, amount).await
    }

    /// Withdraw funds from a [`Signer`] to an address in the given subnet.
    pub async fn withdraw(
        signer: &impl Signer,
        to: Address,
        subnet: EVMSubnet,
        amount: TokenAmount,
    ) -> anyhow::Result<TransactionReceipt> {
        EvmManager::withdraw(signer, to, subnet, amount).await
    }

    /// Transfer funds from [`Signer`] to an address in the given subnet.
    pub async fn transfer(
        signer: &impl Signer,
        to: Address,
        subnet: EVMSubnet,
        amount: TokenAmount,
    ) -> anyhow::Result<TransactionReceipt> {
        EvmManager::transfer(signer, to, subnet, amount).await
    }

    /// Sets or unsets a gas sponsor for the signer.
    pub async fn set_sponsor<C>(
        provider: &impl Provider<C>,
        signer: &mut impl Signer,
        sponsor: Option<Address>,
        options: SetSponsorOptions,
    ) -> anyhow::Result<TxReceipt<()>>
    where
        C: Client + Send + Sync,
    {
        let params = SetSponsorParams {
            from: signer.address(),
            sponsor,
        };
        let params = RawBytes::serialize(params)?;
        signer
            .send_transaction(
                provider,
                BLOBS_ACTOR_ADDR,
                Default::default(),
                SetAccountSponsor as u64,
                params,
                options.gas_params,
                decode_empty,
            )
            .await
    }
}
