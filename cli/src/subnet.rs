// Copyright 2025 Recall Contributors
// SPDX-License-Identifier: Apache-2.0, MIT

use clap::{Args, Subcommand};
use serde_json::json;

use recall_provider::{
    fvm_shared::clock::ChainEpoch, json_rpc::JsonRpcProvider, util::parse_token_credit_rate,
};
use recall_sdk::{
    credits::TokenCreditRate,
    network::NetworkConfig,
    subnet::{SetConfigOptions, Subnet},
    TxParams,
};
use recall_signer::{key::SecretKey, AccountKind, Wallet};

use crate::{parse_secret_key, print_json, print_tx_json, AddressArgs, BroadcastMode, TxArgs};

#[derive(Clone, Debug, Args)]
pub struct SubnetArgs {
    #[command(subcommand)]
    command: SubnetCommands,
}

#[derive(Clone, Debug, Subcommand)]
enum SubnetCommands {
    /// Get the ChainId.
    ChainId,
    /// Get and set the subnet configuration.
    #[command(subcommand)]
    Config(ConfigCommands),
}

#[derive(Clone, Debug, Subcommand)]
enum ConfigCommands {
    /// Set the subnet configuration.
    Set(SetConfigArgs),
    /// Get the current subnet configuration.
    Get(GetConfigArgs),
}

#[derive(Clone, Debug, Args)]
struct SetConfigArgs {
    /// Wallet private key (ECDSA, secp256k1) for signing transactions.
    #[arg(short, long, env = "RECALL_PRIVATE_KEY", value_parser = parse_secret_key, hide_env_values = true)]
    private_key: SecretKey,
    /// The total storage capacity of the subnet.
    #[arg(long)]
    blob_capacity: u64,
    /// The token to credit rate. The amount of atto credits that 1 atto buys.
    #[arg(long, value_parser = parse_token_credit_rate)]
    token_credit_rate: TokenCreditRate,
    /// Block interval at which to debit all credit accounts.
    #[arg(long)]
    blob_credit_debit_interval: ChainEpoch,
    /// The minimum epoch duration a blob can be stored.
    #[arg(long)]
    blob_min_ttl: ChainEpoch,
    /// The default epoch duration a blob is stored.
    #[arg(long)]
    blob_default_ttl: ChainEpoch,
    /// Broadcast mode for the transaction.
    #[arg(short, long, value_enum, env = "RECALL_BROADCAST_MODE", default_value_t = BroadcastMode::Commit)]
    broadcast_mode: BroadcastMode,
    #[command(flatten)]
    tx_args: TxArgs,
}

#[derive(Clone, Debug, Args)]
struct GetConfigArgs {
    #[command(flatten)]
    address: AddressArgs,
}

/// Subnet commands handler.
pub async fn handle_subnet(cfg: NetworkConfig, args: &SubnetArgs) -> anyhow::Result<()> {
    let provider = JsonRpcProvider::new_http(cfg.rpc_url, cfg.subnet_id.chain_id(), None, None)?;

    match &args.command {
        SubnetCommands::ChainId => {
            let chain_id = Subnet::chain_id(provider).await?;
            print_json(&json!({"chain_id": chain_id}))
        }
        SubnetCommands::Config(cmd) => match &cmd {
            ConfigCommands::Set(args) => {
                let broadcast_mode = args.broadcast_mode.get();
                let TxParams {
                    gas_params,
                    sequence,
                } = args.tx_args.to_tx_params();

                let mut signer = Wallet::new_secp256k1(
                    args.private_key.clone(),
                    AccountKind::Ethereum,
                    cfg.subnet_id,
                )?;
                signer.set_sequence(sequence, &provider).await?;

                let tx = Subnet::set_config(
                    &provider,
                    &mut signer,
                    SetConfigOptions {
                        blob_capacity: args.blob_capacity,
                        token_credit_rate: args.token_credit_rate.clone(),
                        blob_credit_debit_interval: args.blob_credit_debit_interval,
                        blob_min_ttl: args.blob_min_ttl,
                        blob_default_ttl: args.blob_default_ttl,
                        broadcast_mode,
                        gas_params,
                    },
                )
                .await?;

                print_tx_json(&tx)
            }
            ConfigCommands::Get(args) => {
                let config = Subnet::get_config(&provider, args.address.height).await?;
                print_json(&json!({
                    "blob_capacity": config.blob_capacity,
                    "token_credit_rate": config.token_credit_rate.to_string(),
                    "blob_credit_debit_interval": config.blob_credit_debit_interval,
                }))
            }
        },
    }
}
