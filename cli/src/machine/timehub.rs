// Copyright 2025 Recall Contributors
// SPDX-License-Identifier: Apache-2.0, MIT

use std::{collections::HashMap, io::Cursor, str::FromStr as _};

use anyhow::{anyhow, Context as _};
use bytes::Bytes;
use cid::Cid;
use clap::{Args, Subcommand};
use clap_stdin::FileOrStdin;
use ethers::utils::hex::ToHexExt;
use recall_provider::{
    fvm_shared::address::Address,
    json_rpc::JsonRpcProvider,
    query::FvmQueryHeight,
    tx::TxStatus,
    util::get_eth_address,
    util::{parse_address, parse_metadata, parse_query_height},
};
use recall_sdk::{
    machine::{
        timehub::{PushOptions, Timehub},
        Machine,
    },
    network::NetworkConfig,
    TxParams,
};
use recall_signer::{
    key::{parse_secret_key, SecretKey},
    AccountKind, Void, Wallet,
};
use serde_json::{json, Value};
use tokio::io::AsyncReadExt;

use crate::{get_address, print_json, print_tx_json, AddressArgs, BroadcastMode, TxArgs};

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
    #[arg(short, long, env = "RECALL_PRIVATE_KEY", value_parser = parse_secret_key, hide_env_values = true)]
    private_key: SecretKey,
    /// Timehub owner address.
    /// The owner defaults to the signer if not specified.
    #[arg(short, long, value_parser = parse_address)]
    owner: Option<Address>,
    /// User-defined metadata.
    #[arg(short, long, value_parser = parse_metadata)]
    metadata: Vec<(String, String)>,
    #[command(flatten)]
    tx_args: TxArgs,
}

#[derive(Clone, Debug, Args)]
struct TimehubPushArgs {
    /// Wallet private key (ECDSA, secp256k1) for signing transactions.
    #[arg(short, long, env = "RECALL_PRIVATE_KEY", value_parser = parse_secret_key, hide_env_values = true)]
    private_key: SecretKey,
    /// Timehub machine address.
    #[arg(short, long, value_parser = parse_address)]
    address: Address,
    /// Input file (or stdin) containing the value to push.
    #[clap(default_value = "-")]
    input: FileOrStdin,
    /// Broadcast mode for the transaction.
    #[arg(short, long, value_enum, env = "RECALL_BROADCAST_MODE", default_value_t = BroadcastMode::Commit)]
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
pub async fn handle_timehub(cfg: NetworkConfig, args: &TimehubArgs) -> anyhow::Result<()> {
    let provider = JsonRpcProvider::new_http(cfg.rpc_url, cfg.subnet_id.chain_id(), None, None)?;
    let subnet_id = cfg.subnet_id;

    match &args.command {
        TimehubCommands::Create(args) => {
            let TxParams {
                sequence,
                gas_params,
            } = args.tx_args.to_tx_params();

            let mut signer =
                Wallet::new_secp256k1(args.private_key.clone(), AccountKind::Ethereum, subnet_id)?;
            signer.set_sequence(sequence, &provider).await?;

            let metadata: HashMap<String, String> = args.metadata.clone().into_iter().collect();

            let (store, tx) =
                Timehub::new(&provider, &mut signer, args.owner, metadata, gas_params).await?;
            let address = store.eth_address()?;

            let tx_json = match &tx.status {
                TxStatus::Pending(tx) => serde_json::to_value(tx)?,
                TxStatus::Committed(receipt) => serde_json::to_value(receipt)?,
            };

            print_json(&json!({"address": address.encode_hex_with_prefix(), "tx": &tx_json}))
        }
        TimehubCommands::List(args) => {
            let address = get_address(args.clone(), &subnet_id)?;
            let metadata = Timehub::list(&provider, &Void::new(address), args.height).await?;

            let metadata = metadata
                .iter()
                .map(|m| {
                    let a = get_eth_address(m.address).expect("invalid address");
                    json!({"address": a.encode_hex_with_prefix(), "kind": m.kind, "metadata" : m.metadata})
                })
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
            let cid = match Cid::read_bytes(Cursor::new(buf.clone())) {
                Ok(cid) => cid,
                Err(_) => {
                    let str_data = String::from_utf8(buf)
                        .context("input should be a multibase encoded utf8 string")?;
                    let str_data = str_data.trim();
                    Cid::from_str(str_data).with_context(|| {
                        format!("'{str_data}' should be a multibase encoded CID")
                    })?
                }
            };

            let payload = Bytes::from(cid.to_bytes());

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

            print_tx_json(&tx)
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
