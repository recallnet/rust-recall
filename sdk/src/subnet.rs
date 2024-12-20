// Copyright 2024 Hoku Contributors
// SPDX-License-Identifier: Apache-2.0, MIT

use fendermint_actor_hoku_config_shared::Method::{GetConfig, SetConfig};
use fendermint_actor_hoku_config_shared::{HokuConfig, SetConfigParams};
use fendermint_vm_actor_interface::hoku_config::HOKU_CONFIG_ACTOR_ADDR;
use tendermint::chain;

use hoku_provider::fvm_shared::{bigint::BigInt, clock::ChainEpoch};
use hoku_provider::json_rpc::JsonRpcProvider;
use hoku_provider::message::{local_message, GasParams, RawBytes};
use hoku_provider::query::{FvmQueryHeight, QueryProvider};
use hoku_provider::response::{decode_as, decode_empty};
use hoku_provider::tx::{BroadcastMode, TxReceipt};
use hoku_provider::{Client, Provider, TendermintClient};
use hoku_signer::Signer;

/// Options for setting config.
#[derive(Clone, Default, Debug)]
pub struct SetConfigOptions {
    /// The total storage capacity of the subnet.
    pub blob_capacity: u64,
    /// The token to credit rate. The amount of credits that 1 atto buys.
    pub token_credit_rate: BigInt,
    /// Block interval at which to debit all credit accounts.
    pub blob_credit_debit_interval: ChainEpoch,
    /// Broadcast mode for the transaction.
    pub broadcast_mode: BroadcastMode,
    /// Gas params for the transaction.
    pub gas_params: GasParams,
}

/// Accessors for fetching subnet-wide information from a node via the CometBFT RPCs.
pub struct Subnet {}

impl Subnet {
    pub async fn chain_id(provider: JsonRpcProvider) -> anyhow::Result<chain::Id> {
        let response = provider.underlying().status().await?;
        Ok(response.node_info.network)
    }

    pub async fn set_config<C>(
        provider: &impl Provider<C>,
        signer: &mut impl Signer,
        options: SetConfigOptions,
    ) -> anyhow::Result<TxReceipt<()>>
    where
        C: Client + Send + Sync,
    {
        let params = SetConfigParams {
            blob_capacity: options.blob_capacity,
            token_credit_rate: options.token_credit_rate,
            blob_credit_debit_interval: options.blob_credit_debit_interval,
        };
        let params = RawBytes::serialize(params)?;
        let message = signer
            .transaction(
                HOKU_CONFIG_ACTOR_ADDR,
                Default::default(),
                SetConfig as u64,
                params,
                options.gas_params,
            )
            .await?;
        provider
            .perform(message, options.broadcast_mode, decode_empty)
            .await
    }

    pub async fn get_config(
        provider: &impl QueryProvider,
        height: FvmQueryHeight,
    ) -> anyhow::Result<HokuConfig> {
        let message = local_message(HOKU_CONFIG_ACTOR_ADDR, GetConfig as u64, Default::default());
        let response = provider.call(message, height, decode_as).await?;
        Ok(response.value)
    }
}
