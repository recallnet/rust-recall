// Copyright 2024 Hoku Contributors
// SPDX-License-Identifier: Apache-2.0, MIT

use clap::{Args, Subcommand};
use ethers::utils::hex::ToHexExt;
use serde_json::json;

use hoku_provider::{
    fvm_shared::address::Address,
    json_rpc::JsonRpcProvider,
    query::FvmQueryHeight,
    util::{get_eth_address, parse_address, parse_query_height},
};
use hoku_sdk::{machine::info, network::NetworkConfig};

use crate::print_json;

pub mod bucket;
pub mod sqlite;
pub mod timehub;

#[derive(Clone, Debug, Args)]
pub struct MachineArgs {
    #[command(subcommand)]
    command: MachineCommands,
}

#[derive(Clone, Debug, Subcommand)]
enum MachineCommands {
    /// Get machine info.
    Info(InfoArgs),
}

#[derive(Clone, Debug, Args)]
struct InfoArgs {
    /// Machine address.
    #[arg(value_parser = parse_address)]
    address: Address,
    /// Query block height.
    /// Possible values:
    /// "committed" (latest committed block),
    /// "pending" (consider pending state changes),
    /// or a specific block height, e.g., "123".
    #[arg(long, value_parser = parse_query_height, default_value = "committed")]
    height: FvmQueryHeight,
}

/// Machine commmands handler.
pub async fn handle_machine(cfg: NetworkConfig, args: &MachineArgs) -> anyhow::Result<()> {
    match &args.command {
        MachineCommands::Info(args) => {
            let provider = JsonRpcProvider::new_http(cfg.rpc_url, None, None)?;
            let metadata = info(&provider, args.address, args.height).await?;
            let owner = get_eth_address(metadata.owner)?.encode_hex_with_prefix();

            print_json(
                &json!({"kind": metadata.kind, "owner": owner, "metadata": metadata.metadata}),
            )
        }
    }
}
