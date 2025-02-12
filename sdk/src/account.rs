// Copyright 2025 Recall Contributors
// SPDX-License-Identifier: Apache-2.0, MIT

use fendermint_actor_blobs_shared::params::{SetAccountStatusParams, SetSponsorParams};
use fendermint_actor_blobs_shared::Method::{SetAccountSponsor, SetAccountStatus};
use fendermint_vm_actor_interface::blobs::BLOBS_ACTOR_ADDR;

pub use fendermint_actor_blobs_shared::state::TtlStatus;

use recall_provider::{
    fvm_ipld_encoding::RawBytes,
    fvm_shared::{address::Address, econ::TokenAmount},
    message::GasParams,
    query::{FvmQueryHeight, QueryProvider},
    response::decode_empty,
    tx::{BroadcastMode, TxResult},
    Client, Provider,
};
use recall_signer::{Signer, SubnetID};

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

/// Options for setting ttl status.
#[derive(Clone, Default, Debug)]
pub struct SetTtlStatusOptions {
    /// TTL status for the account to set.
    pub status: TtlStatus,
    /// Broadcast mode for the transaction.
    pub broadcast_mode: BroadcastMode,
    /// Gas params for the transaction.
    pub gas_params: GasParams,
}

/// A static wrapper around Recall account methods.
pub struct Account {}

impl Account {
    /// Get the sequence (nonce) for a [`Signer`] at the given height.
    pub async fn sequence(
        provider: &impl QueryProvider,
        signer: &impl Signer,
        height: FvmQueryHeight,
    ) -> anyhow::Result<u64> {
        let response = provider.actor_state(&signer.address(), height).await?;
        Ok(response
            .value
            .map(|(_, state)| state.sequence)
            .unwrap_or_default())
    }

    /// Get the balance for a [`Signer`] at the given height.
    pub async fn balance(signer: &impl Signer, subnet: EVMSubnet) -> anyhow::Result<TokenAmount> {
        EvmManager::balance(signer.address(), subnet).await
    }

    /// Get the balance of the supply source (ERC20) for a [`Signer`] at the given height.
    pub async fn supply_source_balance(
        signer: &impl Signer,
        subnet: EVMSubnet,
    ) -> anyhow::Result<TokenAmount> {
        EvmManager::supply_source_balance(signer.address(), subnet).await
    }

    /// Deposit funds from a [`Signer`] to an address in the given subnet.
    pub async fn deposit(
        signer: &impl Signer,
        to: Address,
        from_subnet: EVMSubnet,
        to_subnet: SubnetID,
        amount: TokenAmount,
    ) -> anyhow::Result<TransactionReceipt> {
        // Approve the gateway to spend funds on behalf of the user.
        // This is required when the subnet uses a custom ERC20 token as
        // the gateway's supply source.
        EvmManager::approve_gateway(signer, from_subnet.clone(), amount.clone()).await?;
        EvmManager::deposit(signer, to, from_subnet, to_subnet, amount).await
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
    ) -> anyhow::Result<TxResult<()>>
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
                options.broadcast_mode,
                decode_empty,
            )
            .await
    }

    /// Sets the TTL status for the given account.
    pub async fn set_ttl_status<C>(
        provider: &impl Provider<C>,
        signer: &mut impl Signer,
        account: Address,
        options: SetTtlStatusOptions,
    ) -> anyhow::Result<TxResult<()>>
    where
        C: Client + Send + Sync,
    {
        let params = SetAccountStatusParams {
            subscriber: account,
            status: options.status,
        };
        let params = RawBytes::serialize(params)?;
        signer
            .send_transaction(
                provider,
                BLOBS_ACTOR_ADDR,
                Default::default(),
                SetAccountStatus as u64,
                params,
                options.gas_params,
                options.broadcast_mode,
                decode_empty,
            )
            .await
    }
}
