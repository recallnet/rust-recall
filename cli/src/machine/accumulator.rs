// Copyright 2024 ADM Contributors
// SPDX-License-Identifier: Apache-2.0, MIT

use anyhow::anyhow;
use bytes::Bytes;
use clap::{Args, Subcommand};
use clap_stdin::FileOrStdin;
use fendermint_actor_machine::WriteAccess;
use fendermint_vm_message::query::FvmQueryHeight;
use fvm_shared::address::Address;
use serde_json::json;
use tendermint_rpc::HttpClient;
use tokio::io::AsyncReadExt;

use adm_provider::{json_rpc::JsonRpcProvider, BroadcastMode};
use adm_sdk::machine::{accumulator::Accumulator, Machine};

use crate::{get_signer, print_json, Cli, MAX_ACC_PAYLOAD_SIZE};

#[derive(Clone, Debug, Args)]
pub struct AccumulatorArgs {
    #[command(subcommand)]
    command: AccumulatorCommands,
}

#[derive(Clone, Debug, Subcommand)]
enum AccumulatorCommands {
    Create(AccumulatorCreateArgs),
    Push(AccumulatorPushArgs),
    Root(AccumulatorRootArgs),
}

#[derive(Clone, Debug, Args)]
struct AccumulatorCreateArgs {
    #[arg(long, default_value_t = false)]
    public_write: bool,
}

#[derive(Clone, Debug, Args)]
struct AccumulatorPushArgs {
    #[arg(short, long)]
    address: Address,
    #[clap(default_value = "-")]
    input: FileOrStdin,
}

#[derive(Clone, Debug, Args)]
struct AccumulatorRootArgs {
    #[arg(short, long)]
    address: Address,
}

pub async fn handle_accumulator(cli: Cli, args: &AccumulatorArgs) -> anyhow::Result<()> {
    let provider = JsonRpcProvider::new_http(cli.rpc_url, None)?;

    match &args.command {
        AccumulatorCommands::Create(args) => {
            let mut signer = get_signer(&provider, cli.wallet_pk, cli.chain_name).await?;
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
            let mut signer = get_signer(&provider, cli.wallet_pk, cli.chain_name).await?;
            let machine = Accumulator::<HttpClient>::attach(args.address);

            let mut reader = args.input.into_async_reader().await?;
            let mut buf = Vec::new();
            reader.read_to_end(&mut buf).await?;
            let payload = Bytes::from(buf);

            if payload.len() > MAX_ACC_PAYLOAD_SIZE {
                return Err(anyhow!(
                    "max payload size is {} bytes",
                    MAX_ACC_PAYLOAD_SIZE
                ));
            }

            let tx = machine
                .push(
                    &provider,
                    &mut signer,
                    payload,
                    BroadcastMode::Commit,
                    Default::default(),
                )
                .await?;

            print_json(&tx)
        }
        AccumulatorCommands::Root(args) => {
            let machine = Accumulator::<HttpClient>::attach(args.address);
            let root = machine.root(&provider, FvmQueryHeight::Committed).await?;

            print_json(&json!({"root": root.to_string()}))
        }
    }
}
