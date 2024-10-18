use crate::{get_rpc_url, print_json, Cli};
use clap::{Args, Subcommand};
use hoku_provider::json_rpc::JsonRpcProvider;
use hoku_sdk::chain::Chain;
use serde_json::json;

#[derive(Clone, Debug, Args)]
pub struct ChainArgs {
    #[command(subcommand)]
    command: ChainCommands,
}

#[derive(Clone, Debug, Subcommand)]
enum ChainCommands {
    /// Get the ChainId.
    ChainId,
}

/// Chain commands handler.
pub async fn handle_chain(cli: Cli, args: &ChainArgs) -> anyhow::Result<()> {
    let provider = JsonRpcProvider::new_http(get_rpc_url(&cli)?, None, None)?;

    match &args.command {
        ChainCommands::ChainId => {
            let chain_id = Chain::chain_id(provider).await?;
            print_json(&json!({"chain_id": chain_id}))
        }
    }
}
