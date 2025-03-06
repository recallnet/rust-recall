// Copyright 2025 Recall Contributors
// SPDX-License-Identifier: Apache-2.0, MIT

use std::collections::HashMap;
use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};
use ethers::utils::hex::ToHexExt;
use serde_json::{json, Value};
use tokio::io::{self};

use recall_provider::{
    fvm_shared::{address::Address, clock::ChainEpoch, econ::TokenAmount},
    json_rpc::{JsonRpcProvider, Url},
    query::FvmQueryHeight,
    tx::TxStatus,
    util::{
        get_eth_address, parse_address, parse_metadata, parse_metadata_optional,
        parse_query_height, parse_token_amount,
    },
};
use recall_sdk::machine::bucket::validate_metadata;
use recall_sdk::{
    machine::{
        bucket::{
            AddOptions, Bucket, DeleteOptions, GetOptions, ObjectState, QueryOptions,
            UpdateObjectMetadataOptions,
        },
        Machine,
    },
    network::NetworkConfig,
    TxParams,
};
use recall_signer::{
    key::{parse_secret_key, SecretKey},
    AccountKind, Signer, Void, Wallet,
};


use crate::{get_address, print_json, print_tx_json, AddressArgs, BroadcastMode, TxArgs};

#[derive(Clone, Debug, Args)]
pub struct BucketArgs {
    #[command(subcommand)]
    command: BucketCommands,
}

#[derive(Clone, Debug, Subcommand)]
enum BucketCommands {
    /// Create a new bucket.
    Create(BucketCreateArgs),
    /// List buckets.
    #[clap(alias = "ls")]
    List(AddressArgs),
    /// Add an object with a key prefix.
    Add(BucketAddArgs),
    /// Delete an object.
    Delete(BucketDeleteArgs),
    /// Get an object.
    Get(BucketGetArgs),
    /// Query for objects.
    Query(BucketQueryArgs),
    /// Metadata for objects.
    Metadata(BucketMetadataArgs),
}

#[derive(Clone, Debug, Args)]
struct BucketCreateArgs {
    /// Wallet private key (ECDSA, secp256k1) for signing transactions.
    #[arg(short, long, env = "RECALL_PRIVATE_KEY", value_parser = parse_secret_key, hide_env_values = true)]
    private_key: SecretKey,
    /// Bucket owner address.
    /// The owner defaults to the signer if not specified.
    #[arg(short, long, value_parser = parse_address)]
    owner: Option<Address>,
    /// Shorthand for --metadata "alias=value"
    #[arg(long)]
    alias: Option<String>,
    /// User-defined metadata.
    #[arg(short, long, value_parser = parse_metadata)]
    metadata: Vec<(String, String)>,
    #[command(flatten)]
    tx_args: TxArgs,
}

#[derive(Clone, Debug, Parser)]
struct BucketAddArgs {
    /// Wallet private key (ECDSA, secp256k1) for signing transactions.
    #[arg(short, long, env = "RECALL_PRIVATE_KEY", value_parser = parse_secret_key, hide_env_values = true)]
    private_key: SecretKey,
    /// Node Object API URL.
    #[arg(long, env = "RECALL_OBJECT_API_URL")]
    object_api_url: Option<Url>,
    /// Bucket machine address.
    #[arg(short, long, value_parser = parse_address)]
    address: Address,
    /// Key of the object to upload.
    #[arg(short, long)]
    key: String,
    /// Object time-to-live (TTL) duration.
    /// Credits will be reserved for the duration, after which the object will be deleted.
    /// If not specified, the current default TTL from the config actor is used.
    #[arg(long)]
    ttl: Option<ChainEpoch>,
    /// Overwrite the object if it already exists.
    #[arg(short, long)]
    overwrite: bool,
    /// User-defined metadata.
    #[arg(short, long, value_parser = parse_metadata)]
    metadata: Vec<(String, String)>,
    /// Input file (or stdin) containing the object to upload.
    input: PathBuf,
    /// Amount of tokens to use for inline buying of credits
    #[arg(long, value_parser = parse_token_amount)]
    token_amount: Option<TokenAmount>,
    /// Broadcast mode for the transaction.
    #[arg(short, long, value_enum, env = "RECALL_BROADCAST_MODE", default_value_t = BroadcastMode::Commit)]
    broadcast_mode: BroadcastMode,
    #[command(flatten)]
    tx_args: TxArgs,
}

#[derive(Clone, Debug, Parser)]
struct BucketDeleteArgs {
    /// Wallet private key (ECDSA, secp256k1) for signing transactions.
    #[arg(short, long, env = "RECALL_PRIVATE_KEY", value_parser = parse_secret_key, hide_env_values = true)]
    private_key: SecretKey,
    /// Bucket machine address.
    #[arg(short, long, value_parser = parse_address)]
    address: Address,
    /// Key of the object to delete.
    key: String,
    /// Broadcast mode for the transaction.
    #[arg(short, long, value_enum, env = "RECALL_BROADCAST_MODE", default_value_t = BroadcastMode::Commit)]
    broadcast_mode: BroadcastMode,
    #[command(flatten)]
    tx_args: TxArgs,
}

#[derive(Clone, Debug, Args)]
struct BucketAddressArgs {
    /// Bucket machine address.
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
struct BucketGetArgs {
    /// Node Object API URL.
    #[arg(long, env = "RECALL_OBJECT_API_URL")]
    object_api_url: Option<Url>,
    /// Bucket machine address.
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
struct BucketQueryArgs {
    /// Bucket machine address.
    #[arg(short, long, value_parser = parse_address)]
    address: Address,
    /// The prefix to filter objects by.
    #[arg(short, long, default_value = "")]
    prefix: String,
    /// The delimiter used to define object hierarchy.
    #[arg(short, long, default_value = "/")]
    delimiter: String,
    /// The key from which to start listing objects.
    #[arg(long)]
    start_key: Option<String>,
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

#[derive(Clone, Debug, Args)]
struct BucketMetadataArgs {
    /// Wallet private key (ECDSA, secp256k1) for signing transactions.
    #[arg(short, long, env = "RECALL_PRIVATE_KEY", value_parser = parse_secret_key, hide_env_values = true)]
    private_key: SecretKey,
    /// Bucket machine address.
    #[arg(short, long, value_parser = parse_address)]
    address: Address,
    /// Key of the object to update metadata.
    #[arg(short, long)]
    key: String,
    /// User-defined metadata.
    #[arg(short, long, value_parser = parse_metadata_optional, required=true)]
    metadata: Vec<(String, Option<String>)>,
    /// Broadcast mode for the transaction.
    #[arg(short, long, value_enum, env = "RECALL_BROADCAST_MODE", default_value_t = BroadcastMode::Commit)]
    broadcast_mode: BroadcastMode,
    #[command(flatten)]
    tx_args: TxArgs,
}

/// Bucket commands handler.
pub async fn handle_bucket(
    cfg: NetworkConfig,
    show_progress: bool,
    args: &BucketArgs,
) -> anyhow::Result<()> {
    match &args.command {
        BucketCommands::Create(args) => {
            let provider =
                JsonRpcProvider::new_http(cfg.rpc_url, cfg.subnet_id.chain_id(), None, None)?;

            let TxParams {
                sequence,
                gas_params,
            } = args.tx_args.to_tx_params();

            let mut signer = Wallet::new_secp256k1(
                args.private_key.clone(),
                AccountKind::Ethereum,
                cfg.subnet_id,
            )?;
            signer.set_sequence(sequence, &provider).await?;

            let mut metadata: HashMap<String, String> = args.metadata.clone().into_iter().collect();
            if let Some(alias) = &args.alias {
                metadata.insert("alias".to_string(), alias.clone());
            }

            validate_metadata(&metadata)?;

            let (store, tx) =
                Bucket::new(&provider, &mut signer, args.owner, metadata, gas_params).await?;
            let address = store.eth_address()?;

            let tx_json = match &tx.status {
                TxStatus::Pending(tx) => serde_json::to_value(tx)?,
                TxStatus::Committed(receipt) => serde_json::to_value(receipt)?,
            };

            print_json(&json!({"address": address.encode_hex_with_prefix(), "tx": &tx_json}))
        }
        BucketCommands::List(args) => {
            let provider =
                JsonRpcProvider::new_http(cfg.rpc_url, cfg.subnet_id.chain_id(), None, None)?;

            let address = get_address(args.clone(), &cfg.subnet_id)?;
            let metadata = Bucket::list(&provider, &Void::new(address), args.height).await?;

            let metadata = metadata
                .iter()
                .map(|m| {
                    let a = get_eth_address(m.address).expect("invalid address");
                    json!({"address": a.encode_hex_with_prefix(), "kind": m.kind, "metadata" : m.metadata})
                })
                .collect::<Vec<Value>>();

            print_json(&metadata)
        }
        BucketCommands::Add(args) => {
            let object_api_url = args.object_api_url.clone().unwrap_or(cfg.object_api_url);
            let provider = JsonRpcProvider::new_http(
                cfg.rpc_url,
                cfg.subnet_id.chain_id(),
                None,
                Some(object_api_url),
            )?;

            let broadcast_mode = args.broadcast_mode.get();
            let TxParams {
                sequence,
                gas_params,
            } = args.tx_args.to_tx_params();
            let metadata: HashMap<String, String> = args.metadata.clone().into_iter().collect();

            let mut signer = Wallet::new_secp256k1(
                args.private_key.clone(),
                AccountKind::Ethereum,
                cfg.subnet_id,
            )?;
            signer.set_sequence(sequence, &provider).await?;

            let machine = Bucket::attach(args.address).await?;
            let token_amount = args.token_amount.clone();
            let from = signer.address();
            let tx = machine
                .add_from_path(
                    &provider,
                    &mut signer,
                    from,
                    &args.key,
                    &args.input,
                    AddOptions {
                        ttl: args.ttl,
                        metadata,
                        overwrite: args.overwrite,
                        token_amount,
                        broadcast_mode,
                        gas_params,
                        show_progress,
                    },
                )
                .await?;

            print_tx_json(&tx)
        }
        BucketCommands::Delete(args) => {
            let provider =
                JsonRpcProvider::new_http(cfg.rpc_url, cfg.subnet_id.chain_id(), None, None)?;

            let broadcast_mode = args.broadcast_mode.get();
            let TxParams {
                sequence,
                gas_params,
            } = args.tx_args.to_tx_params();

            let mut signer = Wallet::new_secp256k1(
                args.private_key.clone(),
                AccountKind::Ethereum,
                cfg.subnet_id,
            )?;
            signer.set_sequence(sequence, &provider).await?;

            let machine = Bucket::attach(args.address).await?;
            let from = signer.address();
            let tx = machine
                .delete(
                    &provider,
                    &mut signer,
                    from,
                    &args.key,
                    DeleteOptions {
                        broadcast_mode,
                        gas_params,
                    },
                )
                .await?;

            print_tx_json(&tx)
        }
        BucketCommands::Get(args) => {
            let object_api_url = args.object_api_url.clone().unwrap_or(cfg.object_api_url);
            let provider = JsonRpcProvider::new_http(
                cfg.rpc_url,
                cfg.subnet_id.chain_id(),
                None,
                Some(object_api_url),
            )?;

            let machine = Bucket::attach(args.address).await?;
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
        BucketCommands::Query(args) => {
            let provider =
                JsonRpcProvider::new_http(cfg.rpc_url, cfg.subnet_id.chain_id(), None, None)?;

            let machine = Bucket::attach(args.address).await?;
            let list = machine
                .query(
                    &provider,
                    QueryOptions {
                        prefix: args.prefix.clone(),
                        delimiter: args.delimiter.clone(),
                        start_key: args.start_key.clone().map(|key| key.into_bytes()),
                        limit: args.limit,
                        height: args.height,
                    },
                )
                .await?;

            let objects = list
                .objects
                .iter()
                .map(|(key_bytes, object)| {
                    let key = core::str::from_utf8(key_bytes)
                        .unwrap_or_default()
                        .to_string();
                    json!({"key": key, "value": object_state_to_json(object)})
                })
                .collect::<Vec<Value>>();
            let common_prefixes = list
                .common_prefixes
                .iter()
                .map(|v| Value::String(core::str::from_utf8(v).unwrap_or_default().to_string()))
                .collect::<Vec<Value>>();

            let next_key = match list.next_key {
                Some(key) => {
                    Value::String(core::str::from_utf8(&key).unwrap_or_default().to_string())
                }
                None => Value::Null,
            };

            print_json(
                &json!({"objects": objects, "common_prefixes": common_prefixes, "next_key" : next_key }),
            )
        }
        BucketCommands::Metadata(args) => {
            let provider =
                JsonRpcProvider::new_http(cfg.rpc_url, cfg.subnet_id.chain_id(), None, None)?;

            let broadcast_mode = args.broadcast_mode.get();
            let TxParams {
                sequence,
                gas_params,
            } = args.tx_args.to_tx_params();

            let mut signer = Wallet::new_secp256k1(
                args.private_key.clone(),
                AccountKind::Ethereum,
                cfg.subnet_id,
            )?;
            signer.set_sequence(sequence, &provider).await?;

            let metadata: HashMap<String, Option<String>> =
                args.metadata.clone().into_iter().collect();

            let machine = Bucket::attach(args.address).await?;
            let from = signer.address();
            let tx = machine
                .update_object_metadata(
                    &provider,
                    &mut signer,
                    from,
                    &args.key,
                    metadata,
                    UpdateObjectMetadataOptions {
                        broadcast_mode,
                        gas_params,
                    },
                )
                .await?;

            print_tx_json(&tx)
        }
    }
}

fn object_state_to_json(object: &ObjectState) -> Value {
    let mut val = json!({
        "hash": object.hash.to_string(),
        "size": object.size,
    });
    let obj = val.as_object_mut().unwrap();
    if !object.metadata.is_empty() {
        let mut val = json!({"metadata": object.metadata});
        let meta = val.as_object_mut().unwrap();
        obj.append(meta);
    }
    json!(obj)
}
