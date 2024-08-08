// Copyright 2024 ADM Contributors
// SPDX-License-Identifier: Apache-2.0, MIT

use clap::{Args, Subcommand};
use serde_json::json;

use adm_provider::json_rpc::JsonRpcProvider;
use adm_sdk::storage::Storage;

use crate::{get_address, get_rpc_url, get_subnet_id, print_json, AddressArgs, Cli};

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
pub async fn handle_storage(cli: Cli, args: &StorageArgs) -> anyhow::Result<()> {
    let provider = JsonRpcProvider::new_http(get_rpc_url(&cli)?, None, None)?;
    let subnet_id = get_subnet_id(&cli)?;

    match &args.command {
        StorageCommands::Stats(args) => {
            let stats = Storage::stats(&provider, args.address.height).await?;
            print_json(&json!(stats))
        }
        StorageCommands::Usage(args) => {
            let address = get_address(args.address.clone(), &subnet_id)?;
            let usage = Storage::usage(&provider, address, args.address.height).await?;
            print_json(&json!(usage))
        }
    }
}
