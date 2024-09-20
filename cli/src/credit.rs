// Copyright 2024 Hoku Contributors
// SPDX-License-Identifier: Apache-2.0, MIT

use clap::{Args, Subcommand};
use fendermint_crypto::SecretKey;
use fvm_shared::address::Address;
use fvm_shared::econ::TokenAmount;
use serde_json::json;

use hoku_provider::{
    json_rpc::JsonRpcProvider,
    util::{parse_address, parse_token_amount},
};
use hoku_sdk::credits::{BuyOptions, Credits};
use hoku_sdk::TxParams;
use hoku_signer::{key::parse_secret_key, AccountKind, Signer, Wallet};

use crate::{
    get_address, get_rpc_url, get_subnet_id, print_json, AddressArgs, BroadcastMode, Cli, TxArgs,
};

#[derive(Clone, Debug, Args)]
pub struct CreditArgs {
    #[command(subcommand)]
    command: CreditCommands,
}

#[derive(Clone, Debug, Subcommand)]
enum CreditCommands {
    /// Get subnet-wide credit usage statistics.
    Stats(StatsArgs),
    /// Get credit balance for an account.
    Balance(BalanceArgs),
    /// Buy credits for an account.
    /// Use the `stats` command to see the subnet byte-blocks per atto token rate.
    Buy(BuyArgs),
}

#[derive(Clone, Debug, Args)]
struct StatsArgs {
    #[command(flatten)]
    address: AddressArgs,
}

#[derive(Clone, Debug, Args)]
struct BalanceArgs {
    #[command(flatten)]
    address: AddressArgs,
}

#[derive(Clone, Debug, Args)]
struct BuyArgs {
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

/// Credit commands handler.
pub async fn handle_credit(cli: Cli, args: &CreditArgs) -> anyhow::Result<()> {
    let provider = JsonRpcProvider::new_http(get_rpc_url(&cli)?, None, None)?;
    let subnet_id = get_subnet_id(&cli)?;

    match &args.command {
        CreditCommands::Stats(args) => {
            let stats = Credits::stats(&provider, args.address.height).await?;
            print_json(&json!(stats))
        }
        CreditCommands::Balance(args) => {
            let address = get_address(args.address.clone(), &subnet_id)?;
            let balance = Credits::balance(&provider, address, args.address.height).await?;
            print_json(&json!(balance))
        }
        CreditCommands::Buy(args) => {
            let broadcast_mode = args.broadcast_mode.get();
            let TxParams {
                gas_params,
                sequence,
            } = args.tx_args.to_tx_params();

            let mut signer =
                Wallet::new_secp256k1(args.private_key.clone(), AccountKind::Ethereum, subnet_id)?;
            signer.set_sequence(sequence, &provider).await?;

            let to = args.to.unwrap_or(signer.address());
            let tx = Credits::buy(
                &provider,
                &mut signer,
                to,
                args.amount.clone(),
                BuyOptions {
                    broadcast_mode,
                    gas_params,
                },
            )
            .await?;

            print_json(&tx)
        }
    }
}
