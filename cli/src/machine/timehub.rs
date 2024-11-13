// Copyright 2024 Hoku Contributors
// SPDX-License-Identifier: Apache-2.0, MIT

use anyhow::anyhow;
use bytes::Bytes;
use clap::{Args, Subcommand};
use clap_stdin::FileOrStdin;
use fendermint_actor_machine::WriteAccess;
use fendermint_crypto::SecretKey;
use fendermint_vm_message::query::FvmQueryHeight;
use fvm_shared::address::Address;
use hoku_provider::util::parse_metadata;
use serde_json::{json, Value};
use std::collections::HashMap;
use tokio::io::AsyncReadExt;

use hoku_provider::{
    json_rpc::JsonRpcProvider,
    util::{parse_address, parse_query_height},
};
use hoku_sdk::{
    machine::{
        timehub::{PushOptions, Timehub},
        Machine,
    },
    TxParams,
};
use hoku_signer::{key::parse_secret_key, AccountKind, Void, Wallet};

use crate::{
    get_address, get_rpc_url, get_subnet_id, print_json, AddressArgs, BroadcastMode, Cli, TxArgs,
};

#[derive(Clone, Debug, Args)]
pub struct TimehubArgs {
    #[command(subcommand)]
    command: TimehubCommands,
}

#[derive(Clone, Debug, Subcommand)]
enum TimehubCommands {
    /// Create a new timehub.
    Create(TimehubCreateArgs),
    /// List timehubs.
    #[clap(alias = "ls")]
    List(AddressArgs),
    /// Push a value.
    Push(TimehubPushArgs),
    /// Get leaf at a given index and height.
    Leaf(TimehubLeafArgs),
    /// Get leaf count at a given height.
    Count(TimehubQueryArgs),
    /// Get peaks at a given height.
    Peaks(TimehubQueryArgs),
    /// Get root at a given height.
    Root(TimehubQueryArgs),
}

#[derive(Clone, Debug, Args)]
struct TimehubCreateArgs {
    /// Wallet private key (ECDSA, secp256k1) for signing transactions.
    #[arg(short, long, env = "HOKU_PRIVATE_KEY", value_parser = parse_secret_key)]
    private_key: SecretKey,
    /// Timehub owner address.
    /// The owner defaults to the signer if not specified.
    #[arg(short, long, value_parser = parse_address)]
    owner: Option<Address>,
    /// Allow public write access to the timehub.
    #[arg(long, default_value_t = false)]
    public_write: bool,
    /// User-defined metadata.
    #[arg(short, long, value_parser = parse_metadata)]
    metadata: Vec<(String, String)>,
    #[command(flatten)]
    tx_args: TxArgs,
}

#[derive(Clone, Debug, Args)]
struct TimehubPushArgs {
    /// Wallet private key (ECDSA, secp256k1) for signing transactions.
    #[arg(short, long, env = "HOKU_PRIVATE_KEY", value_parser = parse_secret_key)]
    private_key: SecretKey,
    /// Timehub machine address.
    #[arg(short, long, value_parser = parse_address)]
    address: Address,
    /// Input file (or stdin) containing the value to push.
    #[clap(default_value = "-")]
    input: FileOrStdin,
    /// Broadcast mode for the transaction.
    #[arg(short, long, value_enum, env = "HOKU_BROADCAST_MODE", default_value_t = BroadcastMode::Commit)]
    broadcast_mode: BroadcastMode,
    #[command(flatten)]
    tx_args: TxArgs,
}

#[derive(Clone, Debug, Args)]
struct TimehubQueryArgs {
    /// Timehub machine address.
    #[arg(short, long, value_parser = parse_address)]
    address: Address,
    /// Query block height.
    /// Possible values:
    /// "committed" (latest committed block),
    /// "pending" (consider pending state changes),
    /// or a specific block height, e.g., "123".
    #[arg(long, value_parser = parse_query_height, default_value = "committed")]
    height: FvmQueryHeight,
}

#[derive(Clone, Debug, Args)]
struct TimehubLeafArgs {
    /// Timehub machine address.
    #[arg(short, long, value_parser = parse_address)]
    address: Address,
    /// Leaf index.
    index: u64,
    /// Query block height.
    /// Possible values:
    /// "committed" (latest committed block),
    /// "pending" (consider pending state changes),
    /// or a specific block height, e.g., "123".
    #[arg(long, value_parser = parse_query_height, default_value = "committed")]
    height: FvmQueryHeight,
}

/// Timehub commmands handler.
pub async fn handle_timehub(cli: Cli, args: &TimehubArgs) -> anyhow::Result<()> {
    let provider = JsonRpcProvider::new_http(get_rpc_url(&cli)?, None, None)?;
    let subnet_id = get_subnet_id(&cli)?;

    match &args.command {
        TimehubCommands::Create(args) => {
            let write_access = if args.public_write {
                WriteAccess::Public
            } else {
                WriteAccess::OnlyOwner
            };
            let TxParams {
                sequence,
                gas_params,
            } = args.tx_args.to_tx_params();

            let mut signer =
                Wallet::new_secp256k1(args.private_key.clone(), AccountKind::Ethereum, subnet_id)?;
            signer.set_sequence(sequence, &provider).await?;

            let metadata: HashMap<String, String> = args.metadata.clone().into_iter().collect();

            let (store, tx) = Timehub::new(
                &provider,
                &mut signer,
                args.owner,
                write_access,
                metadata,
                gas_params,
            )
            .await?;

            print_json(&json!({"address": store.address().to_string(), "tx": &tx}))
        }
        TimehubCommands::List(args) => {
            let address = get_address(args.clone(), &subnet_id)?;
            let metadata = Timehub::list(&provider, &Void::new(address), args.height).await?;

            let metadata = metadata
                .iter()
                .map(|m| json!({"address": m.address.to_string(), "kind": m.kind, "metadata": m.metadata}))
                .collect::<Vec<Value>>();

            print_json(&metadata)
        }
        TimehubCommands::Push(args) => {
            let broadcast_mode = args.broadcast_mode.get();
            let TxParams {
                gas_params,
                sequence,
            } = args.tx_args.to_tx_params();

            let mut signer =
                Wallet::new_secp256k1(args.private_key.clone(), AccountKind::Ethereum, subnet_id)?;
            signer.set_sequence(sequence, &provider).await?;

            let mut reader = args.input.into_async_reader().await?;
            let mut buf = Vec::new();
            reader.read_to_end(&mut buf).await?;
            let payload = Bytes::from(buf);

            let machine = Timehub::attach(args.address).await?;
            let tx = machine
                .push(
                    &provider,
                    &mut signer,
                    payload,
                    PushOptions {
                        broadcast_mode,
                        gas_params,
                    },
                )
                .await?;

            print_json(&tx)
        }
        TimehubCommands::Leaf(args) => {
            let machine = Timehub::attach(args.address).await?;
            let leaf = machine
                .leaf(&provider, args.index, args.height)
                .await?
                .ok_or_else(|| {
                    anyhow!(
                        "leaf not found for index '{}' at height {:?}",
                        args.index,
                        args.height
                    )
                })?;

            print_json(&leaf)
        }
        TimehubCommands::Count(args) => {
            let machine = Timehub::attach(args.address).await?;
            let count = machine.count(&provider, args.height).await?;

            print_json(&json!({"count": count}))
        }
        TimehubCommands::Peaks(args) => {
            let machine = Timehub::attach(args.address).await?;
            let peaks = machine.peaks(&provider, args.height).await?;

            print_json(&json!({"peaks": peaks}))
        }
        TimehubCommands::Root(args) => {
            let machine = Timehub::attach(args.address).await?;
            let root = machine.root(&provider, args.height).await?;

            print_json(&json!({"root": root.to_string()}))
        }
    }
}
