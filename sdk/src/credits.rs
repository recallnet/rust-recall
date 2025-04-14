// Copyright 2025 Recall Contributors
// SPDX-License-Identifier: Apache-2.0, MIT

use std::collections::HashMap;

use anyhow::anyhow;
use ethers::utils::hex::ToHexExt;
use recall_fendermint_actor_blobs_shared::params::{
    ApproveCreditParams, BuyCreditParams, GetAccountParams, RevokeCreditParams,
};
use recall_fendermint_actor_blobs_shared::Method::{
    ApproveCredit, BuyCredit, GetAccount, GetStats, RevokeCredit,
};
use recall_fendermint_vm_actor_interface::blobs::BLOBS_ACTOR_ADDR;
use serde::{Deserialize, Serialize};

use recall_provider::{
    fvm_ipld_encoding::{self, RawBytes},
    fvm_shared::{address::Address, clock::ChainEpoch, econ::TokenAmount},
    message::{local_message, GasParams},
    query::{FvmQueryHeight, QueryProvider},
    response::{decode_bytes, decode_empty},
    tx::{BroadcastMode, DeliverTx, TxResult},
    util::get_eth_address,
    {Client, Provider},
};
use recall_signer::Signer;

pub use recall_fendermint_actor_blobs_shared::state::{Credit, TokenCreditRate};

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
    /// Credit approval limit.
    /// If specified, the approval becomes invalid once the used credits reach the
    /// specified limit.
    pub credit_limit: Option<Credit>,
    /// Gas fee limit.
    /// If specified, the approval becomes invalid once the used gas fees reach the
    /// specified limit.
    pub gas_fee_limit: Option<TokenAmount>,
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
    /// Credit approvals to other accounts from this account, keyed by receiver.
    pub approvals_to: HashMap<String, Approval>,
    /// Credit approvals to this account from other accounts, keyed by sender.
    pub approvals_from: HashMap<String, Approval>,
    /// The maximum allowed TTL for actor's blobs.
    pub max_ttl: ChainEpoch,
    /// The total token value an account has used to buy credits.
    pub gas_allowance: String,
}

impl Default for Balance {
    fn default() -> Self {
        Self {
            credit_free: "0".into(),
            credit_committed: "0".into(),
            last_debit_epoch: Some(0),
            credit_sponsor: None,
            approvals_to: HashMap::new(),
            approvals_from: HashMap::new(),
            max_ttl: 0,
            gas_allowance: "0".into(),
        }
    }
}

impl From<recall_fendermint_actor_blobs_shared::state::AccountInfo> for Balance {
    fn from(v: recall_fendermint_actor_blobs_shared::state::AccountInfo) -> Self {
        let last_debit_epoch = if v.last_debit_epoch != 0 {
            Some(v.last_debit_epoch)
        } else {
            None
        };
        Self {
            credit_free: v.credit_free.to_string(),
            credit_committed: v.credit_committed.to_string(),
            last_debit_epoch,
            credit_sponsor: v.credit_sponsor.map(|a| {
                get_eth_address(a)
                    .expect("failed to get ethereum address")
                    .encode_hex_with_prefix()
            }),
            approvals_to: v
                .approvals_to
                .into_iter()
                .map(|(k, v)| {
                    let a = get_eth_address(k).expect("failed to get ethereum address");
                    (a.encode_hex_with_prefix(), v.into())
                })
                .collect(),
            approvals_from: v
                .approvals_from
                .into_iter()
                .map(|(k, v)| {
                    let a = get_eth_address(k).expect("failed to get ethereum address");
                    (a.encode_hex_with_prefix(), v.into())
                })
                .collect(),
            max_ttl: v.max_ttl,
            gas_allowance: v.gas_allowance.to_string(),
        }
    }
}

/// A credit approval.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Approval {
    /// Optional credit approval limit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credit_limit: Option<String>,
    /// Optional gas fee approval limit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gas_fee_limit: Option<String>,
    /// Optional credit approval expiry epoch.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expiry: Option<ChainEpoch>,
    /// Counter for how much credit has been used via this approval.
    pub credit_used: String,
    /// Amount of gas that has been used via this approval.
    pub gas_fee_used: String,
}

impl Default for Approval {
    fn default() -> Self {
        Self {
            credit_limit: None,
            credit_used: "0".into(),
            gas_fee_limit: None,
            gas_fee_used: "0".into(),
            expiry: None,
        }
    }
}

impl From<recall_fendermint_actor_blobs_shared::state::CreditApproval> for Approval {
    fn from(v: recall_fendermint_actor_blobs_shared::state::CreditApproval) -> Self {
        Self {
            credit_limit: v.credit_limit.map(|l| l.to_string()),
            credit_used: v.credit_used.to_string(),
            gas_fee_limit: v.gas_fee_limit.map(|l| l.to_string()),
            gas_fee_used: v.gas_fee_used.to_string(),
            expiry: v.expiry,
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
    /// The token to credit rate.
    pub token_credit_rate: TokenCreditRate,
    // Total number of debit accounts.
    pub num_accounts: u64,
}

impl From<recall_fendermint_actor_blobs_shared::params::GetStatsReturn> for CreditStats {
    fn from(v: recall_fendermint_actor_blobs_shared::params::GetStatsReturn) -> Self {
        Self {
            balance: v.balance.to_string(),
            credit_sold: v.credit_sold.to_string(),
            credit_committed: v.credit_committed.to_string(),
            credit_debited: v.credit_debited.to_string(),
            token_credit_rate: v.token_credit_rate,
            num_accounts: v.num_accounts,
        }
    }
}

/// A static wrapper around Recall credit methods.
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

    /// Buy credits for an account.
    pub async fn buy<C>(
        provider: &impl Provider<C>,
        signer: &mut impl Signer,
        to: Address,
        amount: TokenAmount,
        options: BuyOptions,
    ) -> anyhow::Result<TxResult<Balance>>
    where
        C: Client + Send + Sync,
    {
        let params = BuyCreditParams(to);
        let params = RawBytes::serialize(params)?;
        signer
            .send_transaction(
                provider,
                BLOBS_ACTOR_ADDR,
                amount,
                BuyCredit as u64,
                params,
                options.gas_params,
                options.broadcast_mode,
                decode_buy,
            )
            .await
    }

    /// Approve credits for an account.
    pub async fn approve<C>(
        provider: &impl Provider<C>,
        signer: &mut impl Signer,
        from: Address,
        to: Address,
        options: ApproveOptions,
    ) -> anyhow::Result<TxResult<Approval>>
    where
        C: Client + Send + Sync,
    {
        let params = ApproveCreditParams {
            from,
            to,
            caller_allowlist: None, // TODO: remove this when it's been removed in ipc
            credit_limit: options.credit_limit,
            gas_fee_limit: options.gas_fee_limit,
            ttl: options.ttl,
        };
        let params = RawBytes::serialize(params)?;
        signer
            .send_transaction(
                provider,
                BLOBS_ACTOR_ADDR,
                Default::default(),
                ApproveCredit as u64,
                params,
                options.gas_params,
                options.broadcast_mode,
                decode_approve,
            )
            .await
    }

    /// Revoke credits for an account.
    pub async fn revoke<C>(
        provider: &impl Provider<C>,
        signer: &mut impl Signer,
        from: Address,
        to: Address,
        options: RevokeOptions,
    ) -> anyhow::Result<TxResult<()>>
    where
        C: Client + Send + Sync,
    {
        let params = RevokeCreditParams {
            from,
            to,
            for_caller: None, // TODO: remove this when it's been removed in ipc
        };
        let params = RawBytes::serialize(params)?;
        signer
            .send_transaction(
                provider,
                BLOBS_ACTOR_ADDR,
                Default::default(),
                RevokeCredit as u64,
                params,
                options.gas_params,
                options.broadcast_mode,
                decode_empty,
            )
            .await
    }
}

fn decode_stats(deliver_tx: &DeliverTx) -> anyhow::Result<CreditStats> {
    let data = decode_bytes(deliver_tx)?;
    fvm_ipld_encoding::from_slice::<recall_fendermint_actor_blobs_shared::params::GetStatsReturn>(
        &data,
    )
    .map(|v| v.into())
    .map_err(|e| anyhow!("error parsing as CreditStats: {e}"))
}

fn decode_balance(deliver_tx: &DeliverTx) -> anyhow::Result<Option<Balance>> {
    let data = decode_bytes(deliver_tx)?;
    fvm_ipld_encoding::from_slice::<Option<recall_fendermint_actor_blobs_shared::state::AccountInfo>>(
        &data,
    )
    .map(|v| v.map(|v| v.into()))
    .map_err(|e| anyhow!("error parsing as Option<Balance>: {e}"))
}

fn decode_buy(deliver_tx: &DeliverTx) -> anyhow::Result<Balance> {
    let data = decode_bytes(deliver_tx)?;
    fvm_ipld_encoding::from_slice::<recall_fendermint_actor_blobs_shared::state::AccountInfo>(&data)
        .map(|v| v.into())
        .map_err(|e| anyhow!("error parsing as Balance: {e}"))
}

fn decode_approve(deliver_tx: &DeliverTx) -> anyhow::Result<Approval> {
    let data = decode_bytes(deliver_tx)?;
    fvm_ipld_encoding::from_slice::<recall_fendermint_actor_blobs_shared::state::CreditApproval>(
        &data,
    )
    .map(|v| v.into())
    .map_err(|e| anyhow!("error parsing as CreditApproval: {e}"))
}
