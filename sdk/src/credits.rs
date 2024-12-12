// Copyright 2024 Hoku Contributors
// SPDX-License-Identifier: Apache-2.0, MIT

use std::collections::{HashMap, HashSet};

use anyhow::anyhow;
use fendermint_actor_blobs_shared::params::{
    ApproveCreditParams, BuyCreditParams, GetAccountParams, RevokeCreditParams,
    SetCreditSponsorParams,
};
use fendermint_actor_blobs_shared::Method::{
    ApproveCredit, BuyCredit, GetAccount, GetStats, RevokeCredit, SetCreditSponsor,
};
use fendermint_vm_actor_interface::blobs::BLOBS_ACTOR_ADDR;
use fendermint_vm_message::query::FvmQueryHeight;
use fvm_ipld_encoding::RawBytes;
use fvm_shared::address::Address;
use fvm_shared::bigint::BigUint;
use fvm_shared::clock::ChainEpoch;
use fvm_shared::econ::TokenAmount;
use serde::{Deserialize, Serialize};
use tendermint::abci::response::DeliverTx;
use tendermint_rpc::Client;

use hoku_provider::message::{local_message, GasParams};
use hoku_provider::query::QueryProvider;
use hoku_provider::response::{decode_bytes, decode_empty};
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

/// Options for approving credit.
#[derive(Clone, Default, Debug)]
pub struct ApproveOptions {
    /// Restrict the approval to one or more caller address, e.g., a bucket.
    /// The receiver will only be able to use the approval via a caller contract.
    /// If not set, any caller is allowed.
    pub caller: Option<HashSet<Address>>,
    /// Credit approval limit.
    /// If specified, the approval becomes invalid once the committed credits reach the
    /// specified limit.
    pub limit: Option<BigUint>,
    /// Credit approval time-to-live epochs.
    /// If specified, the approval becomes invalid after this duration.
    pub ttl: Option<ChainEpoch>,
    /// Broadcast mode for the transaction.
    pub broadcast_mode: BroadcastMode,
    /// Gas params for the transaction.
    pub gas_params: GasParams,
}

/// Options for revoking credit.
#[derive(Clone, Default, Debug)]
pub struct RevokeOptions {
    /// Revoke the approval for the caller address.
    /// The address must be part of the existing caller allowlist.
    pub caller: Option<Address>,
    /// Broadcast mode for the transaction.
    pub broadcast_mode: BroadcastMode,
    /// Gas params for the transaction.
    pub gas_params: GasParams,
}

/// Options for setting credit sponsor.
#[derive(Clone, Default, Debug)]
pub struct SetSponsorOptions {
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
    /// Optional default sponsor account address.
    pub credit_sponsor: Option<String>,
    /// The chain epoch of the last debit.
    pub last_debit_epoch: Option<ChainEpoch>,
    /// Credit approvals to other accounts, keyed by receiver, keyed by caller,
    /// which could be the receiver or a specific contract, like a bucket.
    /// This allows for limiting approvals to interactions from a specific contract.
    /// For example, an approval for Alice might be valid for any contract caller, so long as
    /// the origin is Alice.
    /// An approval for Bob might be valid from only one contract caller, so long as
    /// the origin is Bob.
    pub approvals: HashMap<String, Approval>,
}

impl Default for Balance {
    fn default() -> Self {
        Self {
            credit_free: "0".into(),
            credit_committed: "0".into(),
            last_debit_epoch: Some(0),
            credit_sponsor: None,
            approvals: HashMap::new(),
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
            credit_sponsor: v.credit_sponsor.map(|a| a.to_string()),
            approvals: v
                .approvals
                .into_iter()
                .map(|(k, v)| (k.to_string(), v.into()))
                .collect(),
        }
    }
}

/// A credit approval.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Approval {
    /// Optional credit approval limit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<String>,
    /// Optional credit approval expiry epoch.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expiry: Option<ChainEpoch>,
    /// Counter for how much credit has been used via this approval.
    pub used: String,
    /// Optional caller allowlist.
    /// If not present, any caller is allowed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caller_allowlist: Option<HashSet<String>>,
}

impl Default for Approval {
    fn default() -> Self {
        Self {
            limit: None,
            expiry: None,
            used: "0".into(),
            caller_allowlist: None,
        }
    }
}

impl From<fendermint_actor_blobs_shared::state::CreditApproval> for Approval {
    fn from(v: fendermint_actor_blobs_shared::state::CreditApproval) -> Self {
        Self {
            limit: v.limit.map(|l| l.to_string()),
            expiry: v.expiry,
            used: v.used.to_string(),
            caller_allowlist: v
                .caller_allowlist
                .map(|v| v.into_iter().map(|a| a.to_string()).collect()),
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
    pub blob_credits_per_byte_block: u64,
    // Total number of debit accounts.
    // pub num_accounts: u64,
}

impl From<fendermint_actor_blobs_shared::params::GetStatsReturn> for CreditStats {
    fn from(v: fendermint_actor_blobs_shared::params::GetStatsReturn) -> Self {
        Self {
            balance: v.balance.to_string(),
            credit_sold: v.credit_sold.to_string(),
            credit_committed: v.credit_committed.to_string(),
            credit_debited: v.credit_debited.to_string(),
            blob_credits_per_byte_block: v.blob_credits_per_byte_block,
            // num_accounts: v.num_accounts,
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
        from: Address,
        height: FvmQueryHeight,
    ) -> anyhow::Result<Balance> {
        let params = GetAccountParams(from);
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

    pub async fn approve<C>(
        provider: &impl Provider<C>,
        signer: &mut impl Signer,
        from: Address,
        to: Address,
        options: ApproveOptions,
    ) -> anyhow::Result<TxReceipt<Approval>>
    where
        C: Client + Send + Sync,
    {
        let params = ApproveCreditParams {
            from,
            to,
            caller_allowlist: options.caller,
            limit: options.limit,
            ttl: options.ttl,
        };
        let params = RawBytes::serialize(params)?;
        let message = signer
            .transaction(
                BLOBS_ACTOR_ADDR,
                Default::default(),
                ApproveCredit as u64,
                params,
                options.gas_params,
            )
            .await?;
        provider
            .perform(message, options.broadcast_mode, decode_approve)
            .await
    }

    pub async fn revoke<C>(
        provider: &impl Provider<C>,
        signer: &mut impl Signer,
        from: Address,
        to: Address,
        options: RevokeOptions,
    ) -> anyhow::Result<TxReceipt<()>>
    where
        C: Client + Send + Sync,
    {
        let params = RevokeCreditParams {
            from,
            to,
            for_caller: options.caller,
        };
        let params = RawBytes::serialize(params)?;
        let message = signer
            .transaction(
                BLOBS_ACTOR_ADDR,
                Default::default(),
                RevokeCredit as u64,
                params,
                options.gas_params,
            )
            .await?;
        provider
            .perform(message, options.broadcast_mode, decode_empty)
            .await
    }

    pub async fn set_sponsor<C>(
        provider: &impl Provider<C>,
        signer: &mut impl Signer,
        from: Address,
        sponsor: Option<Address>,
        options: SetSponsorOptions,
    ) -> anyhow::Result<TxReceipt<()>>
    where
        C: Client + Send + Sync,
    {
        let params = SetCreditSponsorParams { from, sponsor };
        let params = RawBytes::serialize(params)?;
        let message = signer
            .transaction(
                BLOBS_ACTOR_ADDR,
                Default::default(),
                SetCreditSponsor as u64,
                params,
                options.gas_params,
            )
            .await?;
        provider
            .perform(message, options.broadcast_mode, decode_empty)
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

fn decode_approve(deliver_tx: &DeliverTx) -> anyhow::Result<Approval> {
    let data = decode_bytes(deliver_tx)?;
    fvm_ipld_encoding::from_slice::<fendermint_actor_blobs_shared::state::CreditApproval>(&data)
        .map(|v| v.into())
        .map_err(|e| anyhow!("error parsing as CreditApproval: {e}"))
}
