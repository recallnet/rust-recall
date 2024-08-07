// Copyright 2024 ADM Contributors
// SPDX-License-Identifier: Apache-2.0, MIT

use anyhow::anyhow;
use fendermint_actor_blobs::BuyCreditParams;
use fendermint_actor_blobs::Method::{BuyCredit, GetAccount, GetStats};
use fendermint_vm_actor_interface::blobs::BLOBS_ACTOR_ADDR;
use fendermint_vm_message::query::FvmQueryHeight;
use fvm_ipld_encoding::RawBytes;
use fvm_shared::address::Address;
use fvm_shared::clock::ChainEpoch;
use fvm_shared::econ::TokenAmount;
use serde::{Deserialize, Serialize};
use tendermint::abci::response::DeliverTx;
use tendermint_rpc::Client;

use adm_provider::message::{local_message, GasParams};
use adm_provider::query::QueryProvider;
use adm_provider::response::decode_bytes;
use adm_provider::tx::{BroadcastMode, TxReceipt};
use adm_provider::Provider;
use adm_signer::Signer;

/*
*adm credit stats (subnet-wide summary)
*adm credit fund --to (buy credits by account)
adm credit balance --address (show credit summary by account)

adm storage stats (subnet-wide summary)
adm storage usage --address (see usage by account)
adm storage add --address (add a blob directly)
adm storage get [hash] (get a blob info directly)
adm storage cat [hash] (get a blob directly)
adm storage ls --address (list blobs by account)
 */

/// Options for funding an account.
#[derive(Clone, Default, Debug)]
pub struct FundOptions {
    /// Broadcast mode for the transaction.
    pub broadcast_mode: BroadcastMode,
    /// Gas params for the transaction.
    pub gas_params: GasParams,
}

/// JSON serialization friendly version of [`fendermint_actor_blobs::Account`].
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Account {
    // Total size of all blobs managed by the account.
    // pub capacity_used: String,
    /// Current free credit in byte-blocks that can be used for new commitments.
    pub credit_free: String,
    /// Current committed credit in byte-blocks that will be used for debits.
    pub credit_committed: String,
    /// The chain epoch of the last debit.
    pub last_debit_epoch: Option<ChainEpoch>,
}

impl From<fendermint_actor_blobs::Account> for Account {
    fn from(v: fendermint_actor_blobs::Account) -> Self {
        let last_debit_epoch = if v.last_debit_epoch != 0 {
            Some(v.last_debit_epoch)
        } else {
            None
        };
        Self {
            // capacity_used: v.capacity_used.to_string(),
            credit_free: v.credit_free.to_string(),
            credit_committed: v.credit_committed.to_string(),
            last_debit_epoch,
        }
    }
}

/// JSON serialization friendly version of [`fendermint_actor_blobs::GetSummaryReturn`].
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GetStatsReturn {
    /// The current token balance earned by the subnet.
    pub balance: String,
    // The total free storage capacity of the subnet.
    // pub capacity_free: String,
    // The total used storage capacity of the subnet.
    // pub capacity_used: String,
    /// The total number of credits sold in the subnet.
    pub credit_sold: String,
    /// The total number of credits committed to active storage in the subnet.
    pub credit_committed: String,
    /// The total number of credits debited in the subnet.
    pub credit_debited: String,
    /// The byte-blocks per atto token rate set at genesis.
    pub credit_debit_rate: u64,
    /// Total number of debit accounts.
    pub num_accounts: u64,
    // Total number of actively stored blobs.
    // pub num_blobs: u64,
    // Total number of currently resolving blobs.
    // pub num_resolving: u64,
}

impl From<fendermint_actor_blobs::GetStatsReturn> for GetStatsReturn {
    fn from(v: fendermint_actor_blobs::GetStatsReturn) -> Self {
        Self {
            balance: v.balance.to_string(),
            // capacity_free: v.capacity_free.to_string(),
            // capacity_used: v.capacity_used.to_string(),
            credit_sold: v.credit_sold.to_string(),
            credit_committed: v.credit_committed.to_string(),
            credit_debited: v.credit_debited.to_string(),
            credit_debit_rate: v.credit_debit_rate,
            num_accounts: v.num_accounts,
            // num_blobs: v.num_blobs,
            // num_resolving: v.num_resolving,
        }
    }
}

/// A static wrapper around ADM credit methods.
pub struct Credits {}

impl Credits {
    pub async fn stats(
        provider: &impl QueryProvider,
        height: FvmQueryHeight,
    ) -> anyhow::Result<GetStatsReturn> {
        let message = local_message(BLOBS_ACTOR_ADDR, GetStats as u64, Default::default());
        let response = provider.call(message, height, decode_stats).await?;
        Ok(response.value)
    }

    pub async fn balance(
        provider: &impl QueryProvider,
        height: FvmQueryHeight,
    ) -> anyhow::Result<Account> {
        let message = local_message(BLOBS_ACTOR_ADDR, GetAccount as u64, Default::default());
        let response = provider.call(message, height, decode_account).await?;
        if let Some(account) = response.value {
            Ok(account)
        } else {
            Ok(Account::default())
        }
    }

    pub async fn buy<C>(
        provider: &impl Provider<C>,
        signer: &mut impl Signer,
        address: Address,
        amount: TokenAmount,
        options: FundOptions,
    ) -> anyhow::Result<TxReceipt<Account>>
    where
        C: Client + Send + Sync,
    {
        let params = BuyCreditParams(address);
        let params = RawBytes::serialize(params)?;
        let message = signer
            .transaction(
                BLOBS_ACTOR_ADDR,
                amount,
                BuyCredit as u64,
                params,
                options.gas_params,
            )
            .await?;
        provider
            .perform(message, options.broadcast_mode, decode_buy)
            .await
    }
}

fn decode_stats(deliver_tx: &DeliverTx) -> anyhow::Result<GetStatsReturn> {
    let data = decode_bytes(deliver_tx)?;
    fvm_ipld_encoding::from_slice::<fendermint_actor_blobs::GetStatsReturn>(&data)
        .map(|v| v.into())
        .map_err(|e| anyhow!("error parsing as GetSummaryReturn: {e}"))
}

fn decode_account(deliver_tx: &DeliverTx) -> anyhow::Result<Option<Account>> {
    let data = decode_bytes(deliver_tx)?;
    fvm_ipld_encoding::from_slice::<Option<fendermint_actor_blobs::Account>>(&data)
        .map(|v| v.map(|v| v.into()))
        .map_err(|e| anyhow!("error parsing as Option<Account>: {e}"))
}

fn decode_buy(deliver_tx: &DeliverTx) -> anyhow::Result<Account> {
    let data = decode_bytes(deliver_tx)?;
    fvm_ipld_encoding::from_slice::<fendermint_actor_blobs::Account>(&data)
        .map(|v| v.into())
        .map_err(|e| anyhow!("error parsing as Account: {e}"))
}
