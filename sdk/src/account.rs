// Copyright 2024 Hoku Contributors
// SPDX-License-Identifier: Apache-2.0, MIT

use anyhow::anyhow;
use ethers::prelude::TransactionReceipt;
use fendermint_vm_message::query::FvmQueryHeight;
use fvm_shared::{address::Address, econ::TokenAmount};

use hoku_provider::query::QueryProvider;
use hoku_signer::Signer;

use crate::ipc::{manager::EvmManager, subnet::EVMSubnet};

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
}
