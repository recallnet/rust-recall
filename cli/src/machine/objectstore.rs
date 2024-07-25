// Copyright 2024 ADM Contributors
// SPDX-License-Identifier: Apache-2.0, MIT

use std::path::PathBuf;

use anyhow::anyhow;
use clap::{Args, Parser, Subcommand};
use fendermint_actor_machine::WriteAccess;
use fendermint_crypto::SecretKey;
use fendermint_vm_message::query::FvmQueryHeight;
use fvm_shared::address::Address;
use serde_json::{json, Value};
use std::collections::HashMap;
use tendermint_rpc::Url;
use tokio::fs::File;
use tokio::io::{self};

use adm_provider::{
    json_rpc::JsonRpcProvider,
    util::{parse_address, parse_metadata, parse_query_height},
};
use adm_sdk::machine::objectstore::{AddOptions, DeleteOptions, GetOptions};
use adm_sdk::{
    machine::{
        objectstore::{ObjectStore, QueryOptions},
        Machine,
    },
    TxParams,
};
use adm_signer::{key::parse_secret_key, AccountKind, Void, Wallet};

use crate::{
    get_address, get_rpc_url, get_subnet_id, print_json, AddressArgs, BroadcastMode, Cli, TxArgs,
};

#[derive(Clone, Debug, Args)]
pub struct ObjectstoreArgs {
    #[command(subcommand)]
    command: ObjectstoreCommands,
}

#[derive(Clone, Debug, Subcommand)]
enum ObjectstoreCommands {
    /// Create a new object store.
    Create(ObjectstoreCreateArgs),
    /// List object stores.
    #[clap(alias = "ls")]
    List(AddressArgs),
    /// Add an object with a key prefix.
    Add(ObjectstorePutArgs),
    /// Delete an object.
    Delete(ObjectstoreDeleteArgs),
    /// Get an object.
    Get(ObjectstoreGetArgs),
    /// Query for objects.
    Query(ObjectstoreQueryArgs),
}

#[derive(Clone, Debug, Args)]
struct ObjectstoreCreateArgs {
    /// Wallet private key (ECDSA, secp256k1) for signing transactions.
    #[arg(short, long, env, value_parser = parse_secret_key)]
    private_key: SecretKey,
    /// Allow public write access to the object store.
    #[arg(long, default_value_t = false)]
    public_write: bool,
    #[command(flatten)]
    tx_args: TxArgs,
}

#[derive(Clone, Debug, Parser)]
struct ObjectstorePutArgs {
    /// Wallet private key (ECDSA, secp256k1) for signing transactions.
    #[arg(short, long, env, value_parser = parse_secret_key)]
    private_key: SecretKey,
    /// Node Object API URL.
    #[arg(long, env)]
    object_api_url: Option<Url>,
    /// Object store machine address.
    #[arg(short, long, value_parser = parse_address)]
    address: Address,
    /// Key of the object to upload.
    #[arg(short, long)]
    key: String,
    /// Overwrite the object if it already exists.
    #[arg(short, long)]
    overwrite: bool,
    /// Input file (or stdin) containing the object to upload.
    //#[clap(default_value = "-")]
    input: PathBuf,
    /// Broadcast mode for the transaction.
    #[arg(short, long, value_enum, env, default_value_t = BroadcastMode::Commit)]
    broadcast_mode: BroadcastMode,
    #[command(flatten)]
    tx_args: TxArgs,
    #[arg(short, long, value_parser = parse_metadata)]
    metadata: Vec<(String, String)>,
}

#[derive(Clone, Debug, Parser)]
struct ObjectstoreDeleteArgs {
    /// Wallet private key (ECDSA, secp256k1) for signing transactions.
    #[arg(short, long, env, value_parser = parse_secret_key)]
    private_key: SecretKey,
    /// Object store machine address.
    #[arg(short, long, value_parser = parse_address)]
    address: Address,
    /// Key of the object to delete.
    key: String,
    /// Broadcast mode for the transaction.
    #[arg(short, long, value_enum, env, default_value_t = BroadcastMode::Commit)]
    broadcast_mode: BroadcastMode,
    #[command(flatten)]
    tx_args: TxArgs,
}

#[derive(Clone, Debug, Args)]
struct ObjectstoreAddressArgs {
    /// Object store machine address.
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
struct ObjectstoreGetArgs {
    /// Node Object API URL.
    #[arg(long, env)]
    object_api_url: Option<Url>,
    /// Object store machine address.
    #[arg(short, long, value_parser = parse_address)]
    address: Address,
    /// Key of the object to get.
    key: String,
    /// Range of bytes to get from the object.
    /// Format: "start-end" (inclusive).
    /// Example: "0-99" (first 100 bytes).
    #[arg(short, long)]
    range: Option<String>,
    /// Query block height.
    /// Possible values:
    /// "committed" (latest committed block),
    /// "pending" (consider pending state changes),
    /// or a specific block height, e.g., "123".
    #[arg(long, value_parser = parse_query_height, default_value = "committed")]
    height: FvmQueryHeight,
}

#[derive(Clone, Debug, Args)]
struct ObjectstoreQueryArgs {
    /// Object store machine address.
    #[arg(short, long, value_parser = parse_address)]
    address: Address,
    /// The prefix to filter objects by.
    #[arg(short, long, default_value = "")]
    prefix: String,
    /// The delimiter used to define object hierarchy.
    #[arg(short, long, default_value = "/")]
    delimiter: String,
    /// The offset from which to start listing objects.
    #[arg(short, long, default_value_t = 0)]
    offset: u64,
    /// The maximum number of objects to list. '0' indicates max (10k).
    #[arg(short, long, default_value_t = 0)]
    limit: u64,
    /// Query block height.
    /// Possible values:
    /// "committed" (latest committed block),
    /// "pending" (consider pending state changes),
    /// or a specific block height, e.g., "123".
    #[arg(long, value_parser = parse_query_height, default_value = "committed")]
    height: FvmQueryHeight,
}

/// Objectstore commmands handler.
pub async fn handle_objectstore(cli: Cli, args: &ObjectstoreArgs) -> anyhow::Result<()> {
    let subnet_id = get_subnet_id(&cli)?;

    match &args.command {
        ObjectstoreCommands::Create(args) => {
            let provider = JsonRpcProvider::new_http(get_rpc_url(&cli)?, None, None)?;

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

            let (store, tx) =
                ObjectStore::new(&provider, &mut signer, write_access, gas_params).await?;

            print_json(&json!({"address": store.address().to_string(), "tx": &tx}))
        }
        ObjectstoreCommands::List(args) => {
            let provider = JsonRpcProvider::new_http(get_rpc_url(&cli)?, None, None)?;

            let address = get_address(args.clone(), &subnet_id)?;
            let metadata = ObjectStore::list(&provider, &Void::new(address), args.height).await?;

            let metadata = metadata
                .iter()
                .map(|m| json!({"address": m.address.to_string(), "kind": m.kind}))
                .collect::<Vec<Value>>();

            print_json(&metadata)
        }
        ObjectstoreCommands::Add(args) => {
            let object_api_url = args
                .object_api_url
                .clone()
                .unwrap_or(cli.network.get().object_api_url()?);
            let provider =
                JsonRpcProvider::new_http(get_rpc_url(&cli)?, None, Some(object_api_url))?;

            let broadcast_mode = args.broadcast_mode.get();
            let TxParams {
                sequence,
                gas_params,
            } = args.tx_args.to_tx_params();
            let metadata: HashMap<String, String> = args.metadata.clone().into_iter().collect();

            let mut signer = Wallet::new_secp256k1(
                args.private_key.clone(),
                AccountKind::Ethereum,
                subnet_id.clone(),
            )?;
            signer.set_sequence(sequence, &provider).await?;

            let file = File::open(&args.input).await?;
            let md = file.metadata().await?;
            if !md.is_file() {
                return Err(anyhow!("input must be a file"));
            }

            let machine = ObjectStore::attach(args.address).await?;
            let tx = machine
                .add(
                    &provider,
                    &mut signer,
                    &args.key,
                    file,
                    AddOptions {
                        overwrite: args.overwrite,
                        broadcast_mode,
                        gas_params,
                        show_progress: !cli.quiet,
                        metadata,
                    },
                )
                .await?;

            print_json(&tx)
        }
        ObjectstoreCommands::Delete(args) => {
            let provider = JsonRpcProvider::new_http(get_rpc_url(&cli)?, None, None)?;

            let broadcast_mode = args.broadcast_mode.get();
            let TxParams {
                sequence,
                gas_params,
            } = args.tx_args.to_tx_params();

            let mut signer = Wallet::new_secp256k1(
                args.private_key.clone(),
                AccountKind::Ethereum,
                subnet_id.clone(),
            )?;
            signer.set_sequence(sequence, &provider).await?;

            let machine = ObjectStore::attach(args.address).await?;
            let tx = machine
                .delete(
                    &provider,
                    &mut signer,
                    &args.key,
                    DeleteOptions {
                        broadcast_mode,
                        gas_params,
                    },
                )
                .await?;

            print_json(&tx)
        }
        ObjectstoreCommands::Get(args) => {
            let object_api_url = args
                .object_api_url
                .clone()
                .unwrap_or(cli.network.get().object_api_url()?);
            let provider =
                JsonRpcProvider::new_http(get_rpc_url(&cli)?, None, Some(object_api_url))?;

            let machine = ObjectStore::attach(args.address).await?;
            machine
                .get(
                    &provider,
                    &args.key,
                    io::stdout(),
                    GetOptions {
                        range: args.range.clone(),
                        height: args.height,
                        show_progress: true,
                    },
                )
                .await
        }
        ObjectstoreCommands::Query(args) => {
            let provider = JsonRpcProvider::new_http(get_rpc_url(&cli)?, None, None)?;

            let machine = ObjectStore::attach(args.address).await?;
            let list = machine
                .query(
                    &provider,
                    QueryOptions {
                        prefix: args.prefix.clone(),
                        delimiter: args.delimiter.clone(),
                        offset: args.offset,
                        limit: args.limit,
                        height: args.height,
                    },
                )
                .await?;

            let objects = list
                .objects
                .iter()
                .map(|(key_bytes, object)| {
                    let key = core::str::from_utf8(&key_bytes).unwrap_or_default().to_string();
                    let cid = cid::Cid::try_from(object.cid.clone().0).unwrap_or_default();
                    let value = json!({"cid": cid.to_string(), "resolved": object.resolved, "size": object.size, "metadata": object.metadata});
                    json!({"key": key, "value": value})
                })
                .collect::<Vec<Value>>();
            let common_prefixes = list
                .common_prefixes
                .iter()
                .map(|v| Value::String(core::str::from_utf8(v).unwrap_or_default().to_string()))
                .collect::<Vec<Value>>();

            print_json(&json!({"objects": objects, "common_prefixes": common_prefixes}))
        }
    }
}
