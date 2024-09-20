// Copyright 2024 Hoku Contributors
// SPDX-License-Identifier: Apache-2.0, MIT

use anyhow::anyhow;
use fendermint_actor_blobs_shared::params::{BuyCreditParams, GetAccountParams};
use fendermint_actor_blobs_shared::Method::{BuyCredit, GetAccount, GetStats};
use fendermint_vm_actor_interface::blobs::BLOBS_ACTOR_ADDR;
use fendermint_vm_message::query::FvmQueryHeight;
use fvm_ipld_encoding::RawBytes;
use fvm_shared::address::Address;
use fvm_shared::clock::ChainEpoch;
use fvm_shared::econ::TokenAmount;
use serde::{Deserialize, Serialize};
use tendermint::abci::response::DeliverTx;
use tendermint_rpc::Client;

use hoku_provider::message::{local_message, GasParams};
use hoku_provider::query::QueryProvider;
use hoku_provider::response::decode_bytes;
use hoku_provider::tx::{BroadcastMode, TxReceipt};
use hoku_provider::Provider;
use hoku_signer::Signer;

/// Options for buying credit.
#[derive(Clone, Default, Debug)]
pub struct BuyOptions {
    /// Broadcast mode for the transaction.
    pub broadcast_mode: BroadcastMode,
    /// Gas params for the transaction.
    pub gas_params: GasParams,
}

/// Credit balance for an account.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Balance {
    /// Current free credit in byte-blocks that can be used for new commitments.
    pub credit_free: String,
    /// Current committed credit in byte-blocks that will be used for debits.
    pub credit_committed: String,
    /// The chain epoch of the last debit.
    pub last_debit_epoch: Option<ChainEpoch>,
}

impl Default for Balance {
    fn default() -> Self {
        Self {
            credit_free: "0".into(),
            credit_committed: "0".into(),
            last_debit_epoch: Some(0),
        }
    }
}

impl From<fendermint_actor_blobs_shared::state::Account> for Balance {
    fn from(v: fendermint_actor_blobs_shared::state::Account) -> Self {
        let last_debit_epoch = if v.last_debit_epoch != 0 {
            Some(v.last_debit_epoch)
        } else {
            None
        };
        Self {
            credit_free: v.credit_free.to_string(),
            credit_committed: v.credit_committed.to_string(),
            last_debit_epoch,
        }
    }
}

/// Subnet-wide credit statistics.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreditStats {
    /// The current token balance earned by the subnet.
    pub balance: String,
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
}

impl From<fendermint_actor_blobs_shared::params::GetStatsReturn> for CreditStats {
    fn from(v: fendermint_actor_blobs_shared::params::GetStatsReturn) -> Self {
        Self {
            balance: v.balance.to_string(),
            credit_sold: v.credit_sold.to_string(),
            credit_committed: v.credit_committed.to_string(),
            credit_debited: v.credit_debited.to_string(),
            credit_debit_rate: v.credit_debit_rate,
            num_accounts: v.num_accounts,
        }
    }
}

/// A static wrapper around Hoku credit methods.
pub struct Credits {}

impl Credits {
    pub async fn stats(
        provider: &impl QueryProvider,
        height: FvmQueryHeight,
    ) -> anyhow::Result<CreditStats> {
        let message = local_message(BLOBS_ACTOR_ADDR, GetStats as u64, Default::default());
        let response = provider.call(message, height, decode_stats).await?;
        Ok(response.value)
    }

    pub async fn balance(
        provider: &impl QueryProvider,
        address: Address,
        height: FvmQueryHeight,
    ) -> anyhow::Result<Balance> {
        let params = GetAccountParams(address);
        let params = RawBytes::serialize(params)?;
        let message = local_message(BLOBS_ACTOR_ADDR, GetAccount as u64, params);
        let response = provider.call(message, height, decode_balance).await?;
        if let Some(account) = response.value {
            Ok(account)
        } else {
            Ok(Balance::default())
        }
    }

    pub async fn buy<C>(
        provider: &impl Provider<C>,
        signer: &mut impl Signer,
        to: Address,
        amount: TokenAmount,
        options: BuyOptions,
    ) -> anyhow::Result<TxReceipt<Balance>>
    where
        C: Client + Send + Sync,
    {
        let params = BuyCreditParams(to);
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

fn decode_stats(deliver_tx: &DeliverTx) -> anyhow::Result<CreditStats> {
    let data = decode_bytes(deliver_tx)?;
    fvm_ipld_encoding::from_slice::<fendermint_actor_blobs_shared::params::GetStatsReturn>(&data)
        .map(|v| v.into())
        .map_err(|e| anyhow!("error parsing as CreditStats: {e}"))
}

fn decode_balance(deliver_tx: &DeliverTx) -> anyhow::Result<Option<Balance>> {
    let data = decode_bytes(deliver_tx)?;
    fvm_ipld_encoding::from_slice::<Option<fendermint_actor_blobs_shared::state::Account>>(&data)
        .map(|v| v.map(|v| v.into()))
        .map_err(|e| anyhow!("error parsing as Option<Balance>: {e}"))
}

fn decode_buy(deliver_tx: &DeliverTx) -> anyhow::Result<Balance> {
    let data = decode_bytes(deliver_tx)?;
    fvm_ipld_encoding::from_slice::<fendermint_actor_blobs_shared::state::Account>(&data)
        .map(|v| v.into())
        .map_err(|e| anyhow!("error parsing as Balance: {e}"))
}
