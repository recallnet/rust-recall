use crate::{get_rpc_url, print_json, Cli};
use clap::{Args, Subcommand};
use hoku_provider::json_rpc::JsonRpcProvider;
use hoku_sdk::subnet::Subnet;
use serde_json::json;

#[derive(Clone, Debug, Args)]
pub struct SubnetArgs {
    #[command(subcommand)]
    command: SubnetCommands,
}

#[derive(Clone, Debug, Subcommand)]
enum SubnetCommands {
    /// Get the ChainId.
    ChainId,
}

/// Subnet commands handler.
pub async fn handle_subnet(cli: Cli, args: &SubnetArgs) -> anyhow::Result<()> {
    let provider = JsonRpcProvider::new_http(get_rpc_url(&cli)?, None, None)?;

    match &args.command {
        SubnetCommands::ChainId => {
            let chain_id = Subnet::chain_id(provider).await?;
            print_json(&json!({"chain_id": chain_id}))
        }
    }
}
