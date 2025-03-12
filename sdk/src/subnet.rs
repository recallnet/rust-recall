// Copyright 2025 Recall Contributors
// SPDX-License-Identifier: Apache-2.0, MIT

use fendermint_actor_blobs_shared::state::TokenCreditRate;
use fendermint_actor_recall_config_shared::Method::{GetAdmin, GetConfig, SetAdmin, SetConfig};
use fendermint_actor_recall_config_shared::{RecallConfig, SetAdminParams, SetConfigParams};
use fendermint_vm_actor_interface::recall_config::RECALL_CONFIG_ACTOR_ADDR;
use tendermint::chain;

use recall_provider::{
    fvm_shared::{address::Address, clock::ChainEpoch},
    json_rpc::JsonRpcProvider,
    message::{local_message, GasParams, RawBytes},
    query::{FvmQueryHeight, QueryProvider},
    response::{decode_as, decode_empty},
    tx::{BroadcastMode, TxResult},
    {Client, Provider, TendermintClient},
};
use recall_signer::Signer;

/// Options for setting config admin.
#[derive(Clone, Debug)]
pub struct SetConfigAdminOptions {
    /// Broadcast mode for the transaction.
    pub broadcast_mode: BroadcastMode,
    /// Gas params for the transaction.
    pub gas_params: GasParams,
}

/// Options for setting config.
#[derive(Clone, Default, Debug)]
pub struct SetConfigOptions {
    /// The total storage capacity of the subnet.
    pub blob_capacity: u64,
    /// The token to credit rate.
    pub token_credit_rate: TokenCreditRate,
    /// Block interval at which to debit all credit accounts.
    pub blob_credit_debit_interval: ChainEpoch,
    /// The minimum epoch duration a blob can be stored.
    pub blob_min_ttl: ChainEpoch,
    /// The default epoch duration a blob is stored.
    pub blob_default_ttl: ChainEpoch,
    /// Maximum number of expired blobs to delete in a single batch during debit.
    pub blob_delete_batch_size: u64,
    /// Maximum number of accounts to process in a single batch during debit.
    pub account_debit_batch_size: u64,
    /// Broadcast mode for the transaction.
    pub broadcast_mode: BroadcastMode,
    /// Gas params for the transaction.
    pub gas_params: GasParams,
}

/// Accessors for fetching subnet-wide information from a node via the CometBFT RPCs.
pub struct Subnet {}

impl Subnet {
    /// Returns the chain ID.
    pub async fn chain_id(provider: JsonRpcProvider) -> anyhow::Result<chain::Id> {
        let response = provider.underlying().status().await?;
        Ok(response.node_info.network)
    }

    /// Sets the network config admin.
    pub async fn set_config_admin<C>(
        provider: &impl Provider<C>,
        signer: &mut impl Signer,
        address: Address,
        options: SetConfigAdminOptions,
    ) -> anyhow::Result<TxResult<()>>
    where
        C: Client + Send + Sync,
    {
        let params = SetAdminParams(address);
        let params = RawBytes::serialize(params)?;
        signer
            .send_transaction(
                provider,
                RECALL_CONFIG_ACTOR_ADDR,
                Default::default(),
                SetAdmin as u64,
                params,
                options.gas_params,
                options.broadcast_mode,
                decode_empty,
            )
            .await
    }

    /// Returns the network config admin.
    pub async fn get_config_admin(
        provider: &impl QueryProvider,
        height: FvmQueryHeight,
    ) -> anyhow::Result<Option<Address>> {
        let message = local_message(
            RECALL_CONFIG_ACTOR_ADDR,
            GetAdmin as u64,
            Default::default(),
        );
        let response = provider.call(message, height, decode_as).await?;
        Ok(response.value)
    }

    /// Sets the network config.
    pub async fn set_config<C>(
        provider: &impl Provider<C>,
        signer: &mut impl Signer,
        options: SetConfigOptions,
    ) -> anyhow::Result<TxResult<()>>
    where
        C: Client + Send + Sync,
    {
        let params = SetConfigParams {
            blob_capacity: options.blob_capacity,
            token_credit_rate: options.token_credit_rate,
            blob_credit_debit_interval: options.blob_credit_debit_interval,
            blob_min_ttl: options.blob_min_ttl,
            blob_default_ttl: options.blob_default_ttl,
            blob_delete_batch_size: options.blob_delete_batch_size,
            account_debit_batch_size: options.account_debit_batch_size,
        };
        let params = RawBytes::serialize(params)?;
        signer
            .send_transaction(
                provider,
                RECALL_CONFIG_ACTOR_ADDR,
                Default::default(),
                SetConfig as u64,
                params,
                options.gas_params,
                options.broadcast_mode,
                decode_empty,
            )
            .await
    }

    /// Returns the network config.
    pub async fn get_config(
        provider: &impl QueryProvider,
        height: FvmQueryHeight,
    ) -> anyhow::Result<RecallConfig> {
        let message = local_message(
            RECALL_CONFIG_ACTOR_ADDR,
            GetConfig as u64,
            Default::default(),
        );
        let response = provider.call(message, height, decode_as).await?;
        Ok(response.value)
    }
}
