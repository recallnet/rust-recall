// Copyright 2024 ADM Contributors
// SPDX-License-Identifier: Apache-2.0, MIT

use bytes::Bytes;
use clap::{Args, Subcommand};
use clap_stdin::FileOrStdin;
use fendermint_actor_machine::WriteAccess;
use fendermint_crypto::SecretKey;
use fendermint_vm_message::query::FvmQueryHeight;
use fvm_shared::address::Address;
use serde_json::json;
use tokio::io::AsyncReadExt;

use adm_provider::{json_rpc::JsonRpcProvider, util::parse_address};
use adm_sdk::machine::{accumulator::Accumulator, Machine};
use adm_signer::{key::parse_secret_key, AccountKind, Wallet};

use crate::{get_rpc_url, get_subnet_id, print_json, BroadcastMode, Cli};

#[derive(Clone, Debug, Args)]
pub struct AccumulatorArgs {
    #[command(subcommand)]
    command: AccumulatorCommands,
}

#[derive(Clone, Debug, Subcommand)]
enum AccumulatorCommands {
    /// Create a new accumulator.
    Create(AccumulatorCreateArgs),
    /// Push a new value to the accumulator.
    Push(AccumulatorPushArgs),
    /// Get the current root of the accumulator.
    Root(AccumulatorRootArgs),
}

#[derive(Clone, Debug, Args)]
struct AccumulatorCreateArgs {
    /// Wallet private key (ECDSA, secp256k1) for signing transactions.
    #[arg(short, long, env, value_parser = parse_secret_key)]
    private_key: SecretKey,
    /// Allow public write access to the accumulator.
    #[arg(long, default_value_t = false)]
    public_write: bool,
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
    #[arg(long, value_enum, env, default_value_t = BroadcastMode::Commit)]
    broadcast_mode: BroadcastMode,
}

#[derive(Clone, Debug, Args)]
struct AccumulatorRootArgs {
    /// Accumulator machine address.
    #[arg(short, long, value_parser = parse_address)]
    address: Address,
}

pub async fn handle_accumulator(cli: Cli, args: &AccumulatorArgs) -> anyhow::Result<()> {
    let provider = JsonRpcProvider::new_http(get_rpc_url(&cli)?, None)?;
    let subnet_id = get_subnet_id(&cli)?;

    match &args.command {
        AccumulatorCommands::Create(args) => {
            let mut signer =
                Wallet::new_secp256k1(args.private_key.clone(), AccountKind::Ethereum, subnet_id)?;
            signer.init_sequence(&provider).await?;

            let write_access = if args.public_write {
                WriteAccess::Public
            } else {
                WriteAccess::OnlyOwner
            };
            let (store, tx) =
                Accumulator::new(&provider, &mut signer, write_access, Default::default()).await?;

            print_json(&json!({"address": store.address().to_string(), "tx": &tx}))
        }
        AccumulatorCommands::Push(args) => {
            let mut signer =
                Wallet::new_secp256k1(args.private_key.clone(), AccountKind::Ethereum, subnet_id)?;
            signer.init_sequence(&provider).await?;

            let machine = Accumulator::attach(args.address);

            let mut reader = args.input.into_async_reader().await?;
            let mut buf = Vec::new();
            reader.read_to_end(&mut buf).await?;
            let payload = Bytes::from(buf);

            let broadcast_mode = args.broadcast_mode.get();
            let tx = machine
                .push(
                    &provider,
                    &mut signer,
                    payload,
                    broadcast_mode,
                    Default::default(),
                )
                .await?;

            print_json(&tx)
        }
        AccumulatorCommands::Root(args) => {
            let machine = Accumulator::attach(args.address);
            let root = machine.root(&provider, FvmQueryHeight::Committed).await?;

            print_json(&json!({"root": root.to_string()}))
        }
    }
}
