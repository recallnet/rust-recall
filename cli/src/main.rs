// Copyright 2024 Hoku Contributors
// SPDX-License-Identifier: Apache-2.0, MIT

use clap::{error::ErrorKind, Args, CommandFactory, Parser, Subcommand, ValueEnum};
use fendermint_crypto::SecretKey;
use fendermint_vm_message::query::FvmQueryHeight;
use fvm_shared::{address::Address, econ::TokenAmount};
use serde::Serialize;
use stderrlog::Timestamp;
use tendermint_rpc::Url;

use hoku_provider::{
    message::GasParams,
    tx::BroadcastMode as SDKBroadcastMode,
    util::{parse_address, parse_query_height, parse_token_amount_from_atto},
};
use hoku_sdk::{network::Network as SdkNetwork, TxParams};
use hoku_signer::{key::parse_secret_key, AccountKind, Signer, SubnetID, Wallet};

use crate::account::{handle_account, AccountArgs};
use crate::credit::{handle_credit, CreditArgs};
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
#[command(name = "hoku", author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
    /// Network presets for subnet and RPC URLs.
    #[arg(short, long, env = "HOKU_NETWORK", value_enum, default_value_t = Network::Ignition)]
    network: Network,
    /// The ID of the target subnet.
    #[arg(short, long, env = "HOKU_SUBNET")]
    subnet: Option<SubnetID>,
    /// Node CometBFT RPC URL.
    #[arg(long, env = "HOKU_RPC_URL")]
    rpc_url: Option<Url>,
    /// Logging verbosity (repeat for more verbose logging).
    #[arg(short, long, env = "HOKU_LOG_VERBOSITY", action = clap::ArgAction::Count)]
    verbosity: u8,
    /// Silence logging.
    #[arg(short, long, env = "HOKU_LOG_QUIET", default_value_t = false)]
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
    /// Credit related commands.
    #[clap(alias = "credits")]
    Credit(CreditArgs),
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
    /// Network presets for Ignition testnet.
    Ignition,
}

impl Network {
    pub fn get(&self) -> SdkNetwork {
        match self {
            Network::Mainnet => SdkNetwork::Mainnet,
            Network::Testnet => SdkNetwork::Testnet,
            Network::Localnet => SdkNetwork::Localnet,
            Network::Devnet => SdkNetwork::Devnet,
            Network::Ignition => SdkNetwork::Ignition,
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
    #[arg(long, env = "HOKU_GAS_LIMIT")]
    gas_limit: Option<u64>,
    /// Maximum gas fee for the transaction in attoFIL.
    /// The client will enforce a minimum value of 100 attoFIL.
    /// 1FIL = 10**18 attoFIL.
    #[arg(long, env = "HOKU_GAS_FEE_CAP", value_parser = parse_token_amount_from_atto)]
    gas_fee_cap: Option<TokenAmount>,
    /// Gas premium for the transaction in attoFIL.
    /// The client will enforce a minimum value of 100,000 attoFIL.
    /// 1FIL = 10**18 attoFIL.
    #[arg(long, env = "HOKU_GAS_PREMIUM", value_parser = parse_token_amount_from_atto)]
    gas_premium: Option<TokenAmount>,
    /// Gas sponsor address.
    #[arg(long, env = "HOKU_GAS_SPONSOR", value_parser = parse_address)]
    gas_sponsor: Option<Address>,
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
                gas_limit: self.gas_limit.unwrap_or(fvm_shared::BLOCK_GAS_LIMIT),
                gas_fee_cap: self.gas_fee_cap.clone().unwrap_or_default(),
                gas_premium: self.gas_premium.clone().unwrap_or_default(),
                gas_sponsor: self.gas_sponsor,
            },
        }
    }
}

#[derive(Clone, Debug, Args)]
struct AddressArgs {
    /// Wallet private key (ECDSA, secp256k1) for signing transactions.
    #[arg(short, long, env = "HOKU_PRIVATE_KEY", value_parser = parse_secret_key)]
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

    stderrlog::new()
        .module(module_path!())
        .quiet(cli.quiet)
        .verbosity(cli.verbosity as usize)
        .timestamp(Timestamp::Millisecond)
        .init()?;

    cli.network.get().init();

    match &cli.command.clone() {
        Commands::Account(args) => handle_account(cli, args).await,
        Commands::Subnet(args) => handle_subnet(cli, args).await,
        Commands::Credit(args) => handle_credit(cli, args).await,
        Commands::Storage(args) => handle_storage(cli, args).await,
        Commands::Bucket(args) => handle_bucket(cli, args).await,
        Commands::Timehub(args) => handle_timehub(cli, args).await,
        Commands::Machine(args) => handle_machine(cli, args).await,
    }
}

/// Returns address from private key or address arg.
fn get_address(args: AddressArgs, subnet_id: &SubnetID) -> anyhow::Result<Address> {
    let address = if let Some(addr) = args.address {
        addr
    } else if let Some(sk) = args.private_key.clone() {
        let signer = Wallet::new_secp256k1(sk, AccountKind::Ethereum, subnet_id.clone(), None)?;
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

/// Returns subnet ID from the override or network preset.
fn get_subnet_id(cli: &Cli) -> anyhow::Result<SubnetID> {
    Ok(cli.subnet.clone().unwrap_or(cli.network.get().subnet_id()?))
}

/// Returns rpc url from the override or network preset.
fn get_rpc_url(cli: &Cli) -> anyhow::Result<Url> {
    Ok(cli.rpc_url.clone().unwrap_or(cli.network.get().rpc_url()?))
}

/// Print serializable to stdout as pretty formatted JSON.
fn print_json<T: Serialize>(value: &T) -> anyhow::Result<()> {
    let json = serde_json::to_string_pretty(&value)?;
    println!("{}", json);
    Ok(())
}
