// Copyright 2024 Hoku Contributors
// SPDX-License-Identifier: Apache-2.0, MIT
use std::collections::HashMap;

use anyhow::bail;
use clap::{Args, Subcommand};
use hoku_provider::fvm_shared::address::Address;
use hoku_provider::query::FvmQueryHeight;
use hoku_provider::util::parse_metadata;
use hoku_provider::{
    json_rpc::JsonRpcProvider,
    util::{parse_address, parse_query_height},
};
use hoku_sdk::network::NetworkConfig;
use hoku_sdk::{
    machine::{
        sqlite::{ExecuteOptions, QueryReturn, Sqlite},
        Machine,
    },
    TxParams,
};
use hoku_signer::key::SecretKey;
use hoku_signer::{key::parse_secret_key, AccountKind, Void, Wallet};
use serde_json::{json, Value};

use crate::{get_address, print_json, AddressArgs, BroadcastMode, TxArgs};

#[derive(Clone, Debug, Args)]
pub struct SqliteArgs {
    #[command(subcommand)]
    command: SqliteCommands,
}

#[derive(Clone, Debug, Subcommand)]
enum SqliteCommands {
    /// Create a new sqlite machine.
    Create(CreateArgs),
    /// List sqlite machines
    #[clap(alias = "ls")]
    List(AddressArgs),
    /// Execute a sqlite query
    Query(QueryArgs),
    /// Execute a sqlite statement (e.g. insert)
    Execute(ExecuteArgs),
}

#[derive(Clone, Debug, Args)]
struct CreateArgs {
    /// Wallet private key (ECDSA, secp256k1) for signing transactions.
    #[arg(short, long, env = "HOKU_PRIVATE_KEY", value_parser = parse_secret_key, hide_env_values = true)]
    private_key: SecretKey,
    /// Sqlite owner address.
    /// The owner defaults to the signer if not specified.
    #[arg(short, long, value_parser = parse_address)]
    owner: Option<Address>,
    /// Allow public write access to the sqlite database.
    #[arg(long, default_value_t = false)]
    public_write: bool,
    /// User-defined metadata.
    #[arg(short, long, value_parser = parse_metadata)]
    metadata: Vec<(String, String)>,
    #[command(flatten)]
    tx_args: TxArgs,
}

#[derive(Clone, Debug, Args)]
struct QueryArgs {
    /// Sqlite machine address.
    #[arg(short, long, value_parser = parse_address)]
    address: Address,
    /// Query SQL statement
    #[arg(short, long)]
    query: String,
    /// Query block height.
    /// Possible values:
    /// "committed" (latest committed block),
    /// "pending" (consider pending state changes),
    /// or a specific block height, e.g., "123".
    #[arg(long, value_parser = parse_query_height, default_value = "committed")]
    height: FvmQueryHeight,
}

#[derive(Clone, Debug, Args)]
struct ExecuteArgs {
    /// Wallet private key (ECDSA, secp256k1) for signing transactions.
    #[arg(short, long, env = "HOKU_PRIVATE_KEY", value_parser = parse_secret_key, hide_env_values = true)]
    private_key: SecretKey,
    /// Timehub machine address.
    #[arg(short, long, value_parser = parse_address)]
    address: Address,
    /// Comma delimited array of SQL statements to execute
    #[arg(short, long, use_value_delimiter = true, value_delimiter = '\n')]
    statements: Vec<String>,
    /// Broadcast mode for the transaction.
    #[arg(short, long, value_enum, env, default_value_t = BroadcastMode::Commit)]
    broadcast_mode: BroadcastMode,
    #[command(flatten)]
    tx_args: TxArgs,
}

/// Sqlite commmands handler.
pub async fn handle_sqlite(cfg: NetworkConfig, args: &SqliteArgs) -> anyhow::Result<()> {
    let provider = JsonRpcProvider::new_http(cfg.rpc_url, None, None)?;
    let subnet_id = cfg.subnet_id;

    match args.command.clone() {
        SqliteCommands::Create(args) => {
            let TxParams {
                sequence,
                gas_params,
            } = args.tx_args.to_tx_params();

            let mut signer =
                Wallet::new_secp256k1(args.private_key.clone(), AccountKind::Ethereum, subnet_id)?;
            signer.set_sequence(sequence, &provider).await?;

            let metadata: HashMap<String, String> = args.metadata.clone().into_iter().collect();

            let (store, tx) =
                Sqlite::new(&provider, &mut signer, args.owner, metadata, gas_params).await?;

            print_json(&json!({"address": store.address().to_string(), "tx": &tx}))
        }
        SqliteCommands::List(args) => {
            let address = get_address(args.clone(), &subnet_id)?;
            let metadata = Sqlite::list(&provider, &Void::new(address), args.height).await?;

            let metadata = metadata
                .iter()
                .map(|m| json!({"address": m.address.to_string(), "kind": m.kind, "metadata": m.metadata}))
                .collect::<Vec<Value>>();

            print_json(&metadata)
        }
        SqliteCommands::Query(args) => {
            let machine = Sqlite::attach(args.address).await?;
            let res: QueryReturn = machine.query(&provider, args.query, args.height).await?;

            print_json(&res)
        }
        SqliteCommands::Execute(args) => {
            if args.statements.is_empty() {
                bail!("statement to execute is required");
            }
            let broadcast_mode = args.broadcast_mode.get();
            let TxParams {
                gas_params,
                sequence,
            } = args.tx_args.to_tx_params();

            let mut signer =
                Wallet::new_secp256k1(args.private_key.clone(), AccountKind::Ethereum, subnet_id)?;
            signer.set_sequence(sequence, &provider).await?;

            let machine = Sqlite::attach(args.address).await?;
            let tx = machine
                .execute(
                    &provider,
                    &mut signer,
                    args.statements,
                    ExecuteOptions {
                        broadcast_mode,
                        gas_params,
                    },
                )
                .await?;

            print_json(&tx)
        }
    }
}
