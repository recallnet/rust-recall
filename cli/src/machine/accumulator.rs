// Copyright 2024 Hoku Contributors
// SPDX-License-Identifier: Apache-2.0, MIT

use std::collections::HashMap;

use bytes::Bytes;
use clap::{Args, Subcommand};
use clap_stdin::FileOrStdin;
use fendermint_actor_machine::WriteAccess;
use fendermint_crypto::SecretKey;
use fendermint_vm_message::query::FvmQueryHeight;
use fvm_shared::address::Address;
use hoku_provider::util::parse_metadata;
use serde_json::{json, Value};
use tokio::io::{self, AsyncReadExt, AsyncWriteExt};

use hoku_provider::{
    json_rpc::JsonRpcProvider,
    util::{parse_address, parse_query_height},
};
use hoku_sdk::{
    machine::{
        accumulator::{Accumulator, PushOptions},
        Machine,
    },
    TxParams,
};
use hoku_signer::{key::parse_secret_key, AccountKind, Void, Wallet};

use crate::{
    get_address, get_rpc_url, get_subnet_id, print_json, AddressArgs, BroadcastMode, Cli, TxArgs,
};

#[derive(Clone, Debug, Args)]
pub struct AccumulatorArgs {
    #[command(subcommand)]
    command: AccumulatorCommands,
}

#[derive(Clone, Debug, Subcommand)]
enum AccumulatorCommands {
    /// Create a new accumulator.
    Create(AccumulatorCreateArgs),
    /// List accumulators.
    #[clap(alias = "ls")]
    List(AddressArgs),
    /// Push a value.
    Push(AccumulatorPushArgs),
    /// Get leaf at a given index and height.
    Leaf(AccumulatorLeafArgs),
    /// Get leaf count at a given height.
    Count(AccumulatorQueryArgs),
    /// Get peaks at a given height.
    Peaks(AccumulatorQueryArgs),
    /// Get root at a given height.
    Root(AccumulatorQueryArgs),
}

#[derive(Clone, Debug, Args)]
struct AccumulatorCreateArgs {
    /// Wallet private key (ECDSA, secp256k1) for signing transactions.
    #[arg(short, long, env, value_parser = parse_secret_key)]
    private_key: SecretKey,
    /// Allow public write access to the accumulator.
    #[arg(long, default_value_t = false)]
    public_write: bool,
    /// User-defined metadata.
    #[arg(short, long, value_parser = parse_metadata)]
    metadata: Vec<(String, String)>,
    #[command(flatten)]
    tx_args: TxArgs,
}

#[derive(Clone, Debug, Args)]
struct AccumulatorPushArgs {
    /// Wallet private key (ECDSA, secp256k1) for signing transactions.
    #[arg(short, long, env, value_parser = parse_secret_key)]
    private_key: SecretKey,
    /// Accumulator machine address.
    #[arg(short, long, value_parser = parse_address)]
    address: Address,
    /// Input file (or stdin) containing the value to push.
    #[clap(default_value = "-")]
    input: FileOrStdin,
    /// Broadcast mode for the transaction.
    #[arg(short, long, value_enum, env, default_value_t = BroadcastMode::Commit)]
    broadcast_mode: BroadcastMode,
    #[command(flatten)]
    tx_args: TxArgs,
}

#[derive(Clone, Debug, Args)]
struct AccumulatorQueryArgs {
    /// Accumulator machine address.
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
struct AccumulatorLeafArgs {
    /// Accumulator machine address.
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

/// Accumulator commmands handler.
pub async fn handle_accumulator(cli: Cli, args: &AccumulatorArgs) -> anyhow::Result<()> {
    let provider = JsonRpcProvider::new_http(get_rpc_url(&cli)?, None, None)?;
    let subnet_id = get_subnet_id(&cli)?;

    match &args.command {
        AccumulatorCommands::Create(args) => {
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

            let (store, tx) =
                Accumulator::new(&provider, &mut signer, write_access, metadata, gas_params)
                    .await?;

            print_json(&json!({"address": store.address().to_string(), "tx": &tx}))
        }
        AccumulatorCommands::List(args) => {
            let address = get_address(args.clone(), &subnet_id)?;
            let metadata = Accumulator::list(&provider, &Void::new(address), args.height).await?;

            let metadata = metadata
                .iter()
                .map(|m| json!({"address": m.address.to_string(), "kind": m.kind, "metadata": m.metadata}))
                .collect::<Vec<Value>>();

            print_json(&metadata)
        }
        AccumulatorCommands::Push(args) => {
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

            let machine = Accumulator::attach(args.address).await?;
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
        AccumulatorCommands::Leaf(args) => {
            let machine = Accumulator::attach(args.address).await?;
            let leaf = machine.leaf(&provider, args.index, args.height).await?;

            let mut stdout = io::stdout();
            stdout.write_all(&leaf).await?;
            Ok(())
        }
        AccumulatorCommands::Count(args) => {
            let machine = Accumulator::attach(args.address).await?;
            let count = machine.count(&provider, args.height).await?;

            print_json(&json!({"count": count}))
        }
        AccumulatorCommands::Peaks(args) => {
            let machine = Accumulator::attach(args.address).await?;
            let peaks = machine.peaks(&provider, args.height).await?;

            print_json(&json!({"peaks": peaks}))
        }
        AccumulatorCommands::Root(args) => {
            let machine = Accumulator::attach(args.address).await?;
            let root = machine.root(&provider, args.height).await?;

            print_json(&json!({"root": root.to_string()}))
        }
    }
}
