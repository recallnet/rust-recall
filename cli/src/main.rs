// Copyright 2025 Recall Contributors
// SPDX-License-Identifier: Apache-2.0, MIT

use std::collections::HashSet;

use clap::{error::ErrorKind, Args, CommandFactory, Parser, Subcommand, ValueEnum};
use serde::Serialize;
use stderrlog::Timestamp;

use recall_provider::{
    fvm_shared::{address::Address, econ::TokenAmount},
    json_rpc::Url,
    message::GasParams,
    query::FvmQueryHeight,
    tx::{BroadcastMode as SDKBroadcastMode, TxResult, TxStatus},
    util::{parse_address, parse_query_height, parse_token_amount_from_atto},
};
use recall_sdk::{network::Network as SdkNetwork, TxParams};
use recall_signer::{
    key::{parse_secret_key, SecretKey},
    AccountKind, Signer, SubnetID, Wallet,
};

use crate::account::{handle_account, AccountArgs};
use crate::machine::{
    bucket::{handle_bucket, BucketArgs},
    handle_machine,
    timehub::{handle_timehub, TimehubArgs},
    MachineArgs,
};
use crate::storage::{handle_storage, StorageArgs};
use crate::subnet::{handle_subnet, SubnetArgs};

mod account;
mod credit;
mod machine;
mod storage;
mod subnet;

#[derive(Clone, Debug, Parser)]
#[command(name = "recall", author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
    /// Network presets for subnet and RPC URLs.
    #[arg(short, long, env = "RECALL_NETWORK", value_enum, default_value_t = Network::Testnet)]
    network: Network,
    /// The ID of the target subnet.
    #[arg(short, long, env = "RECALL_SUBNET")]
    subnet: Option<SubnetID>,
    /// Node CometBFT RPC URL.
    #[arg(long, env = "RECALL_RPC_URL")]
    rpc_url: Option<Url>,
    /// Logging verbosity (repeat for more verbose logging).
    #[arg(short, long, env = "RECALL_LOG_VERBOSITY", action = clap::ArgAction::Count)]
    verbosity: u8,
    /// Silence logging.
    #[arg(short, long, env = "RECALL_LOG_QUIET", default_value_t = false)]
    quiet: bool,
}

#[derive(Clone, Debug, Subcommand)]
#[allow(clippy::large_enum_variant)]
enum Commands {
    /// Account related commands.
    #[clap(alias = "accounts")]
    Account(AccountArgs),
    /// Subnet related commands.
    Subnet(SubnetArgs),
    /// Storage related commands.
    Storage(StorageArgs),
    /// Machine related commands.
    #[clap(alias = "machines")]
    Machine(MachineArgs),
    /// Bucket related commands (alias: bu).
    #[clap(alias = "bu")]
    Bucket(BucketArgs),
    /// Timehub related commands (alias: th).
    #[clap(alias = "th")]
    Timehub(TimehubArgs),
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
enum Network {
    /// Network presets for mainnet.
    Mainnet,
    /// Network presets for Calibration (default pre-mainnet).
    Testnet,
    /// Network presets for a local three-node network.
    Localnet,
    /// Network presets for local development.
    Devnet,
}

impl Network {
    pub fn get(&self) -> SdkNetwork {
        match self {
            Network::Mainnet => SdkNetwork::Mainnet,
            Network::Testnet => SdkNetwork::Testnet,
            Network::Localnet => SdkNetwork::Localnet,
            Network::Devnet => SdkNetwork::Devnet,
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
enum BroadcastMode {
    /// Return immediately after the transaction is broadcasted without waiting for check results.
    Async,
    /// Wait for the check results before returning from broadcast.
    Sync,
    /// Wait for the delivery results before returning from broadcast.
    Commit,
}

impl BroadcastMode {
    pub fn get(&self) -> SDKBroadcastMode {
        match self {
            BroadcastMode::Async => SDKBroadcastMode::Async,
            BroadcastMode::Sync => SDKBroadcastMode::Sync,
            BroadcastMode::Commit => SDKBroadcastMode::Commit,
        }
    }
}

#[derive(Clone, Debug, Args)]
struct TxArgs {
    /// Gas limit for the transaction.
    #[arg(long, env = "RECALL_GAS_LIMIT")]
    gas_limit: Option<u64>,
    /// Maximum gas fee for the transaction in attoRECALL.
    /// The client will enforce a minimum value of 100 attoRECALL.
    /// 1RECALL = 10**18 attoRECALL.
    #[arg(long, env = "RECALL_GAS_FEE_CAP", value_parser = parse_token_amount_from_atto)]
    gas_fee_cap: Option<TokenAmount>,
    /// Gas premium for the transaction in attoRECALL.
    /// The client will enforce a minimum value of 100,000 attoRECALL.
    /// 1RECALL = 10**18 attoRECALL.
    #[arg(long, env = "RECALL_GAS_PREMIUM", value_parser = parse_token_amount_from_atto)]
    gas_premium: Option<TokenAmount>,
    /// Sequence for the transaction.
    #[arg(long)]
    sequence: Option<u64>,
}

impl TxArgs {
    /// Creates transaction params from tx related CLI arguments.
    pub fn to_tx_params(&self) -> TxParams {
        TxParams {
            sequence: self.sequence,
            gas_params: GasParams {
                gas_limit: self.gas_limit.unwrap_or_default(),
                gas_fee_cap: self.gas_fee_cap.clone().unwrap_or_default(),
                gas_premium: self.gas_premium.clone().unwrap_or_default(),
            },
        }
    }
}

#[derive(Clone, Debug, Args)]
struct AddressArgs {
    /// Wallet private key (ECDSA, secp256k1) for signing transactions.
    #[arg(short, long, env = "RECALL_PRIVATE_KEY", value_parser = parse_secret_key, hide_env_values = true)]
    private_key: Option<SecretKey>,
    /// Account address. The signer address is used if no address is given.
    #[arg(short, long, value_parser = parse_address)]
    address: Option<Address>,
    /// Query block height.
    /// Possible values:
    /// "committed" (latest committed block),
    /// "pending" (consider pending state changes),
    /// or a specific block height, e.g., "123".
    #[arg(long, value_parser = parse_query_height, default_value = "committed")]
    height: FvmQueryHeight,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let verbosity = cli.verbosity as usize;

    stderrlog::new()
        .module(module_path!())
        .quiet(cli.quiet)
        .verbosity(verbosity)
        .timestamp(Timestamp::Millisecond)
        .init()?;

    let cfg = cli.network.get().get_config();

    match &cli.command.clone() {
        Commands::Account(args) => handle_account(cfg, args, verbosity).await,
        Commands::Subnet(args) => handle_subnet(cfg, args).await,
        Commands::Storage(args) => handle_storage(cfg, args).await,
        Commands::Bucket(args) => handle_bucket(cfg, !cli.quiet, args).await,
        Commands::Timehub(args) => handle_timehub(cfg, args).await,
        Commands::Machine(args) => handle_machine(cfg, args).await,
    }
}

/// Returns address from private key or address arg.
fn get_address(args: AddressArgs, subnet_id: &SubnetID) -> anyhow::Result<Address> {
    let address = if let Some(addr) = args.address {
        addr
    } else if let Some(sk) = args.private_key.clone() {
        let signer = Wallet::new_secp256k1(sk, AccountKind::Ethereum, subnet_id.clone())?;
        signer.address()
    } else {
        Cli::command()
            .error(
                ErrorKind::MissingRequiredArgument,
                "the following required arguments were not provided: --private-key OR --address",
            )
            .exit();
    };
    Ok(address)
}

/// Parser function for comma-separated address values.
pub fn parse_address_list(s: &str) -> anyhow::Result<HashSet<Address>> {
    s.split(',')
        .map(|s| parse_address(s).map_err(|e| anyhow::anyhow!("error parsing address: {}", e)))
        .collect::<Result<HashSet<_>, _>>()
}

/// Print serializable to stdout as pretty formatted JSON.
fn print_json<T: Serialize>(value: &T) -> anyhow::Result<()> {
    let json = serde_json::to_string_pretty(&value)?;
    println!("{}", json);
    Ok(())
}

/// Print serializable to stdout as pretty formatted JSON.
fn print_tx_json<T: 'static>(tx_res: &TxResult<T>) -> anyhow::Result<()> {
    let json = match &tx_res.status {
        TxStatus::Pending(tx) => serde_json::to_string_pretty(tx)?,
        TxStatus::Committed(receipt) => serde_json::to_string_pretty(receipt)?,
    };
    println!("{}", json);
    Ok(())
}
