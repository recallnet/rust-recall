// Copyright 2024 Hoku Contributors
// SPDX-License-Identifier: Apache-2.0, MIT

use clap::{Args, Subcommand};
use fendermint_crypto::SecretKey;
use fvm_shared::address::Address;
use fvm_shared::bigint::BigUint;
use fvm_shared::clock::ChainEpoch;
use fvm_shared::econ::TokenAmount;
use serde_json::json;

use hoku_provider::{
    json_rpc::JsonRpcProvider,
    util::{parse_address, parse_credit_amount, parse_token_amount},
};
use hoku_sdk::TxParams;
use hoku_sdk::{
    credits::{ApproveOptions, BuyOptions, Credits, RevokeOptions},
    network::NetworkConfig,
};
use hoku_signer::{key::parse_secret_key, AccountKind, Signer, Wallet};

use crate::{get_address, print_json, AddressArgs, BroadcastMode, TxArgs};

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
    /// Approve an account to use credits from another acccount.
    Approve(ApproveArgs),
    /// Revoke an account from using credits from another acccount.
    Revoke(RevokeArgs),
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
    #[arg(short, long, env = "HOKU_PRIVATE_KEY", value_parser = parse_secret_key)]
    private_key: SecretKey,
    /// The recipient account address. If not present, the signer address is used.
    #[arg(long, value_parser = parse_address)]
    recipient: Option<Address>,
    /// The amount of FIL to spend.
    #[arg(value_parser = parse_token_amount)]
    amount: TokenAmount,
    /// Broadcast mode for the transaction.
    #[arg(short, long, value_enum, env = "HOKU_BROADCAST_MODE", default_value_t = BroadcastMode::Commit)]
    broadcast_mode: BroadcastMode,
    #[command(flatten)]
    tx_args: TxArgs,
}

#[derive(Clone, Debug, Args)]
struct ApproveArgs {
    /// Wallet private key (ECDSA, secp256k1) for signing transactions.
    #[arg(short, long, env = "HOKU_PRIVATE_KEY", value_parser = parse_secret_key)]
    private_key: SecretKey,
    /// The receiver account address.
    #[arg(long, value_parser = parse_address)]
    receiver: Address,
    /// Restrict the approval to a caller address, e.g., a bucket.
    /// The receiver will only be able to use the approval via a caller contract.
    #[arg(long, value_parser = parse_address)]
    caller: Option<Address>,
    /// Credit approval limit.
    /// If specified, the approval becomes invalid once the committed credits reach the
    /// specified limit.
    #[arg(long, value_parser = parse_credit_amount)]
    limit: Option<BigUint>,
    /// Credit approval time-to-live epochs.
    /// If specified, the approval becomes invalid after this duration.
    #[arg(long)]
    ttl: Option<ChainEpoch>,
    /// Broadcast mode for the transaction.
    #[arg(short, long, value_enum, env = "HOKU_BROADCAST_MODE", default_value_t = BroadcastMode::Commit)]
    broadcast_mode: BroadcastMode,
    #[command(flatten)]
    tx_args: TxArgs,
}

#[derive(Clone, Debug, Args)]
struct RevokeArgs {
    /// Wallet private key (ECDSA, secp256k1) for signing transactions.
    #[arg(short, long, env = "HOKU_PRIVATE_KEY", value_parser = parse_secret_key)]
    private_key: SecretKey,
    /// The receiver account address.
    #[arg(long, value_parser = parse_address)]
    receiver: Address,
    /// Restrict the approval to a caller address, e.g., a bucket.
    /// The receiver will only be able to use the approval via a caller contract.
    #[arg(long, value_parser = parse_address)]
    caller: Option<Address>,
    /// Broadcast mode for the transaction.
    #[arg(short, long, value_enum, env = "HOKU_BROADCAST_MODE", default_value_t = BroadcastMode::Commit)]
    broadcast_mode: BroadcastMode,
    #[command(flatten)]
    tx_args: TxArgs,
}

/// Credit commands handler.
pub async fn handle_credit(cfg: NetworkConfig, args: &CreditArgs) -> anyhow::Result<()> {
    let provider = JsonRpcProvider::new_http(cfg.rpc_url, None, None)?;

    match &args.command {
        CreditCommands::Stats(args) => {
            let stats = Credits::stats(&provider, args.address.height).await?;
            print_json(&json!(stats))
        }
        CreditCommands::Balance(args) => {
            let address = get_address(args.address.clone(), &cfg.subnet_id)?;
            let balance = Credits::balance(&provider, address, args.address.height).await?;
            print_json(&json!(balance))
        }
        CreditCommands::Buy(args) => {
            let broadcast_mode = args.broadcast_mode.get();
            let TxParams {
                gas_params,
                sequence,
            } = args.tx_args.to_tx_params();

            let mut signer = Wallet::new_secp256k1(
                args.private_key.clone(),
                AccountKind::Ethereum,
                cfg.subnet_id,
            )?;
            signer.set_sequence(sequence, &provider).await?;

            let recipient = args.recipient.unwrap_or(signer.address());
            let tx = Credits::buy(
                &provider,
                &mut signer,
                recipient,
                args.amount.clone(),
                BuyOptions {
                    broadcast_mode,
                    gas_params,
                },
            )
            .await?;

            print_json(&tx)
        }
        CreditCommands::Approve(args) => {
            let broadcast_mode = args.broadcast_mode.get();
            let TxParams {
                gas_params,
                sequence,
            } = args.tx_args.to_tx_params();

            let mut signer = Wallet::new_secp256k1(
                args.private_key.clone(),
                AccountKind::Ethereum,
                cfg.subnet_id,
            )?;
            signer.set_sequence(sequence, &provider).await?;

            let from = signer.address();
            let tx = Credits::approve(
                &provider,
                &mut signer,
                from,
                args.receiver,
                ApproveOptions {
                    caller: args.caller,
                    limit: args.limit.clone(),
                    ttl: args.ttl,
                    broadcast_mode,
                    gas_params,
                },
            )
            .await?;

            print_json(&tx)
        }
        CreditCommands::Revoke(args) => {
            let broadcast_mode = args.broadcast_mode.get();
            let TxParams {
                gas_params,
                sequence,
            } = args.tx_args.to_tx_params();

            let mut signer = Wallet::new_secp256k1(
                args.private_key.clone(),
                AccountKind::Ethereum,
                cfg.subnet_id,
            )?;
            signer.set_sequence(sequence, &provider).await?;

            let from = signer.address();
            let tx = Credits::revoke(
                &provider,
                &mut signer,
                from,
                args.receiver,
                RevokeOptions {
                    caller: args.caller,
                    broadcast_mode,
                    gas_params,
                },
            )
            .await?;

            print_json(&tx)
        }
    }
}
