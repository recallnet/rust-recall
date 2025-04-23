// Copyright 2025 Recall Contributors
// SPDX-License-Identifier: Apache-2.0, MIT

use clap::{Args, Subcommand};
use recall_provider::json_rpc::JsonRpcProvider;
use recall_sdk::{network::NetworkConfig, storage::Storage};
use serde_json::json;

use crate::{get_address, print_json, AddressArgs};

#[derive(Clone, Debug, Args)]
pub struct StorageArgs {
    #[command(subcommand)]
    command: StorageCommands,
}

#[derive(Clone, Debug, Subcommand)]
enum StorageCommands {
    /// Get subnet-wide storage usage statistics.
    Stats(StatsArgs),
    /// Get storage usage for an account.
    Usage(UsageArgs),
}

#[derive(Clone, Debug, Args)]
struct StatsArgs {
    #[command(flatten)]
    address: AddressArgs,
}

#[derive(Clone, Debug, Args)]
struct UsageArgs {
    #[command(flatten)]
    address: AddressArgs,
}

/// Storage commands handler.
pub async fn handle_storage(cfg: NetworkConfig, args: &StorageArgs) -> anyhow::Result<()> {
    let provider = JsonRpcProvider::new_http(cfg.rpc_url, cfg.subnet_id.chain_id(), None, None)?;

    match &args.command {
        StorageCommands::Stats(args) => {
            let stats = Storage::stats(&provider, args.address.height).await?;
            print_json(&json!(stats))
        }
        StorageCommands::Usage(args) => {
            let address = get_address(args.address.clone(), &cfg.subnet_id)?;
            let usage = Storage::usage(&provider, address, args.address.height).await?;
            print_json(&json!(usage))
        }
    }
}
