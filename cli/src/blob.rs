// Copyright 2024 ADM Contributors
// SPDX-License-Identifier: Apache-2.0, MIT

use clap::{Args, Subcommand};
use fendermint_crypto::SecretKey;
use fvm_shared::address::Address;
use fvm_shared::econ::TokenAmount;
use serde_json::json;

use adm_provider::{
    json_rpc::JsonRpcProvider,
    util::{parse_address, parse_token_amount},
};
use adm_sdk::blobs::{Blobs, FundOptions};
use adm_sdk::TxParams;
use adm_signer::{key::parse_secret_key, AccountKind, Signer, Wallet};

use crate::{get_rpc_url, get_subnet_id, print_json, AddressArgs, BroadcastMode, Cli, TxArgs};

#[derive(Clone, Debug, Args)]
pub struct BlobArgs {
    #[command(subcommand)]
    command: BlobCommands,
}

#[derive(Clone, Debug, Subcommand)]
enum BlobCommands {
    /// Get the status of blobs.
    Status(StatusArgs),
    Fund(FundArgs),
}

#[derive(Clone, Debug, Args)]
struct StatusArgs {
    #[command(flatten)]
    address: AddressArgs,
}

#[derive(Clone, Debug, Args)]
struct FundArgs {
    /// Wallet private key (ECDSA, secp256k1) for signing transactions.
    #[arg(short, long, env, value_parser = parse_secret_key)]
    private_key: SecretKey,
    /// The recipient account address. If not present, the signer address is used.
    #[arg(long, value_parser = parse_address)]
    to: Option<Address>,
    /// The amount of FIL to spend.
    #[arg(value_parser = parse_token_amount)]
    amount: TokenAmount,
    /// Broadcast mode for the transaction.
    #[arg(short, long, value_enum, env, default_value_t = BroadcastMode::Commit)]
    broadcast_mode: BroadcastMode,
    #[command(flatten)]
    tx_args: TxArgs,
}

/// Blob commands handler.
pub async fn handle_blob(cli: Cli, args: &BlobArgs) -> anyhow::Result<()> {
    let provider = JsonRpcProvider::new_http(get_rpc_url(&cli)?, None, None)?;
    let subnet_id = get_subnet_id(&cli)?;

    match &args.command {
        BlobCommands::Status(args) => {
            let status = Blobs::query(&provider, args.address.height).await?;

            print_json(&json!({"status": status}))
        }
        BlobCommands::Fund(args) => {
            let broadcast_mode = args.broadcast_mode.get();
            let TxParams {
                gas_params,
                sequence,
            } = args.tx_args.to_tx_params();

            let mut signer =
                Wallet::new_secp256k1(args.private_key.clone(), AccountKind::Ethereum, subnet_id)?;
            signer.set_sequence(sequence, &provider).await?;

            let to = args.to.unwrap_or(signer.address());
            let tx = Blobs::fund(
                &provider,
                &mut signer,
                to,
                args.amount.clone(),
                FundOptions {
                    broadcast_mode,
                    gas_params,
                },
            )
            .await?;

            print_json(&tx)
        }
    }
}
