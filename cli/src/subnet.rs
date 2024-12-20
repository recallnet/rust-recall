// Copyright 2024 Hoku Contributors
// SPDX-License-Identifier: Apache-2.0, MIT

use clap::{Args, Subcommand};
use serde_json::json;

use hoku_provider::{
    fvm_shared::{bigint::BigInt, clock::ChainEpoch},
    json_rpc::JsonRpcProvider,
};
use hoku_sdk::subnet::SetConfigOptions;
use hoku_sdk::{network::NetworkConfig, subnet::Subnet, TxParams};
use hoku_signer::key::SecretKey;
use hoku_signer::{AccountKind, Wallet};

use crate::{parse_secret_key, print_json, AddressArgs, BroadcastMode, TxArgs};

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
    #[arg(short, long, env = "HOKU_PRIVATE_KEY", value_parser = parse_secret_key)]
    private_key: SecretKey,
    /// The total storage capacity of the subnet.
    #[arg(long)]
    blob_capacity: u64,
    /// The token to credit rate. The amount of atto credits that 1 atto buys.
    #[arg(long)]
    token_credit_rate: BigInt,
    /// Block interval at which to debit all credit accounts.
    #[arg(long)]
    blob_credit_debit_interval: ChainEpoch,
    /// Broadcast mode for the transaction.
    #[arg(short, long, value_enum, env = "HOKU_BROADCAST_MODE", default_value_t = BroadcastMode::Commit)]
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
    let provider = JsonRpcProvider::new_http(cfg.rpc_url, None, None)?;

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
                        broadcast_mode,
                        gas_params,
                    },
                )
                .await?;

                print_json(&tx)
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
