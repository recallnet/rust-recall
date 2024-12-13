// Copyright 2024 Hoku Contributors
// SPDX-License-Identifier: Apache-2.0, MIT

use std::collections::HashSet;

use clap::{Args, Subcommand};
use serde_json::json;

use hoku_provider::{
    fvm_shared::{address::Address, clock::ChainEpoch, econ::TokenAmount},
    json_rpc::JsonRpcProvider,
    util::{parse_address, parse_credit_amount, parse_token_amount, parse_token_amount_from_atto},
};
use hoku_sdk::{
    credits::{ApproveOptions, BuyOptions, Credit, Credits, RevokeOptions, SetSponsorOptions},
    network::NetworkConfig,
    TxParams,
};
use hoku_signer::{
    key::{parse_secret_key, SecretKey},
    AccountKind, Signer, Wallet,
};

use crate::{get_address, parse_address_list, print_json, AddressArgs, BroadcastMode, TxArgs};

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
    /// Use the `stats` command to see the subnet credit per atto token rate.
    Buy(BuyArgs),
    /// Approve an account to use credits from another acccount.
    Approve(ApproveArgs),
    /// Revoke an account from using credits from another acccount.
    Revoke(RevokeArgs),
    /// Sponsor related commands.
    #[command(subcommand)]
    Sponsor(SponsorCommands),
}

#[derive(Clone, Debug, Subcommand)]
enum SponsorCommands {
    /// Set account default credit sponsor for gas fees.
    /// This will have no effect on gas fees if the required credit approval does not exist.
    Set(SetSponsorArgs),
    /// Unset account default credit sponsor for gas fees.
    Unset(UnsetSponsorArgs),
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
    to: Option<Address>,
    /// The amount of HOKU to spend.
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
    to: Address,
    /// Restrict the approval to one or more caller address, e.g., a bucket.
    /// The receiver will only be able to use the approval via a caller contract.
    /// If not set, any caller is allowed.
    #[arg(long, value_parser = parse_address_list)]
    caller: Option<HashSet<Address>>,
    /// Credit approval limit.
    /// If specified, the approval becomes invalid once the committed credits reach the
    /// specified limit.
    #[arg(long, value_parser = parse_credit_amount)]
    credit_limit: Option<Credit>,
    /// Gas fee limit.
    /// If specified, the approval becomes invalid once the commited gas reach the
    /// specified limit.
    #[arg(long, value_parser = parse_token_amount_from_atto)]
    gas_fee_limit: Option<TokenAmount>,
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
    to: Address,
    /// Revoke the approval for the caller address.
    /// The address must be part of the existing caller allowlist.
    #[arg(long, value_parser = parse_address)]
    caller: Option<Address>,
    /// Broadcast mode for the transaction.
    #[arg(short, long, value_enum, env = "HOKU_BROADCAST_MODE", default_value_t = BroadcastMode::Commit)]
    broadcast_mode: BroadcastMode,
    #[command(flatten)]
    tx_args: TxArgs,
}

#[derive(Clone, Debug, Args)]
struct SetSponsorArgs {
    /// Wallet private key (ECDSA, secp256k1) for signing transactions.
    #[arg(short, long, env = "HOKU_PRIVATE_KEY", value_parser = parse_secret_key)]
    private_key: SecretKey,
    /// Credit sponsor address.
    #[arg(value_parser = parse_address)]
    sponsor: Address,
    /// Broadcast mode for the transaction.
    #[arg(short, long, value_enum, env = "HOKU_BROADCAST_MODE", default_value_t = BroadcastMode::Commit)]
    broadcast_mode: BroadcastMode,
    #[command(flatten)]
    tx_args: TxArgs,
}

#[derive(Clone, Debug, Args)]
struct UnsetSponsorArgs {
    /// Wallet private key (ECDSA, secp256k1) for signing transactions.
    #[arg(short, long, env = "HOKU_PRIVATE_KEY", value_parser = parse_secret_key)]
    private_key: SecretKey,
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
                args.to,
                ApproveOptions {
                    caller: args.caller.clone(),
                    credit_limit: args.credit_limit.clone(),
                    gas_fee_limit: args.gas_fee_limit.clone(),
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
                args.to,
                RevokeOptions {
                    caller: args.caller,
                    broadcast_mode,
                    gas_params,
                },
            )
            .await?;

            print_json(&tx)
        }
        CreditCommands::Sponsor(cmd) => match cmd {
            SponsorCommands::Set(args) => {
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
                let tx = Credits::set_sponsor(
                    &provider,
                    &mut signer,
                    from,
                    Some(args.sponsor),
                    SetSponsorOptions {
                        broadcast_mode,
                        gas_params,
                    },
                )
                .await?;

                print_json(&tx)
            }
            SponsorCommands::Unset(args) => {
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
                let tx = Credits::set_sponsor(
                    &provider,
                    &mut signer,
                    from,
                    None,
                    SetSponsorOptions {
                        broadcast_mode,
                        gas_params,
                    },
                )
                .await?;

                print_json(&tx)
            }
        },
    }
}
