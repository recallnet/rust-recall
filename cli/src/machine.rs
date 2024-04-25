// Copyright 2024 ADM Contributors
// SPDX-License-Identifier: Apache-2.0, MIT

use clap::{Args, Subcommand};
use fendermint_vm_message::query::FvmQueryHeight;
use fvm_shared::address::Address;

use adm_provider::json_rpc::JsonRpcProvider;
use adm_sdk::Adm;

use crate::machine::{
    accumulator::{handle_accumulator, AccumulatorArgs},
    objectstore::{handle_objectstore, ObjectstoreArgs},
};
use crate::{print_json, Cli};

pub mod accumulator;
pub mod objectstore;

#[derive(Clone, Debug, Args)]
pub struct MachineArgs {
    #[command(subcommand)]
    command: MachineCommands,
}

#[derive(Clone, Debug, Subcommand)]
enum MachineCommands {
    /// Get machine metadata at a specific address.
    Get(GetMachineArgs),
    /// List machine metadata for a specific owner.
    List(ListMachineArgs),
    /// Interact with an object store type machine.
    Objectstore(ObjectstoreArgs),
    /// Interact with the accumulator type machine.
    Accumulator(AccumulatorArgs),
}

#[derive(Clone, Debug, Args)]
struct GetMachineArgs {
    #[arg(short, long)]
    address: Address,
}

#[derive(Clone, Debug, Args)]
struct ListMachineArgs {
    #[arg(short, long)]
    owner: Address,
}

pub async fn handle_machine(cli: Cli, args: &MachineArgs) -> anyhow::Result<()> {
    match &args.command {
        MachineCommands::Get(args) => {
            let provider = JsonRpcProvider::new_http(cli.rpc_url, None)?;
            let metadata =
                Adm::get_machine_metadata(&provider, args.address, FvmQueryHeight::Committed)
                    .await?;

            print_json(&metadata)
        }
        MachineCommands::List(args) => {
            let provider = JsonRpcProvider::new_http(cli.rpc_url, None)?;
            let metadata =
                Adm::list_machine_metadata(&provider, args.owner, FvmQueryHeight::Committed)
                    .await?;

            print_json(&metadata)
        }
        MachineCommands::Objectstore(args) => handle_objectstore(cli, args).await,
        MachineCommands::Accumulator(args) => handle_accumulator(cli, args).await,
    }
}
