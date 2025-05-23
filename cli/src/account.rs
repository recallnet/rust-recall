// Copyright 2025 Recall Contributors
// SPDX-License-Identifier: Apache-2.0, MIT

use std::time::Duration;

use anyhow::anyhow;
use clap::{Args, Subcommand, ValueEnum};
use recall_provider::{
    fvm_shared::{address::Address, econ::TokenAmount},
    json_rpc::JsonRpcProvider,
    util::{get_eth_address, parse_address, parse_token_amount},
};
use recall_sdk::{
    account::AccountStatus as SdkAccountStatus,
    account::{Account, SetSponsorOptions, SetStatusOptions},
    credits::{Balance, Credits},
    ipc::subnet::EVMSubnet,
    network::{NetworkConfig, ParentNetworkConfig},
    TxParams,
};
use recall_signer::{
    key::{parse_secret_key, random_secretkey, SecretKey},
    AccountKind, EthAddress, Signer, SubnetID, Void, Wallet,
};
use reqwest::Url;
use serde_json::{json, Value};

use crate::credit::{handle_credit, CreditArgs};
use crate::{get_address, print_json, print_tx_json, AddressArgs, BroadcastMode, TxArgs};

#[derive(Clone, Debug, Args)]
pub struct AccountArgs {
    #[command(subcommand)]
    command: AccountCommands,
}

#[derive(Clone, Debug, Subcommand)]
enum AccountCommands {
    /// Create a new local wallet from a random seed (wallet details are NOT sent to the network).
    Create,
    /// Get account information.
    Info(InfoArgs),
    /// Deposit funds into a subnet from its parent.
    Deposit(FundArgs),
    /// Withdraw funds from a subnet to its parent.
    Withdraw(FundArgs),
    /// Transfer funds to another account in a subnet.
    Transfer(TransferArgs),
    /// Sponsor related commands.
    #[command(subcommand)]
    Sponsor(SponsorCommands),
    /// Credit related commands.
    Credit(CreditArgs),
    /// Set account status.
    SetStatus(SetStatusArgs),
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
struct SubnetArgs {
    /// The Ethereum API rpc http endpoint.
    #[arg(long)]
    evm_rpc_url: Option<Url>,
    /// Timeout for calls to the Ethereum API.
    #[arg(long, value_parser = humantime::parse_duration, default_value = "60s")]
    evm_rpc_timeout: Duration,
    /// Bearer token for any Authorization header.
    #[arg(long)]
    evm_rpc_auth_token: Option<String>,
    /// The gateway contract address.
    #[arg(long, value_parser = parse_address)]
    evm_gateway: Option<Address>,
    /// The registry contract address.
    #[arg(long, value_parser = parse_address)]
    evm_registry: Option<Address>,
    /// The supply source contract address.
    #[arg(long, value_parser = parse_address)]
    evm_supply_source: Option<Address>,
}

#[derive(Clone, Debug, Args)]
struct InfoArgs {
    #[command(flatten)]
    address: AddressArgs,
    #[command(flatten)]
    subnet: SubnetArgs,
}

#[derive(Clone, Debug, Args)]
struct FundArgs {
    /// Wallet private key (ECDSA, secp256k1) for signing transactions.
    #[arg(short, long, env = "RECALL_PRIVATE_KEY", value_parser = parse_secret_key, hide_env_values = true)]
    private_key: SecretKey,
    /// The recipient account address. If not present, the signer address is used.
    #[arg(long, value_parser = parse_address)]
    to: Option<Address>,
    /// The amount to transfer in FIL.
    #[arg(value_parser = parse_token_amount)]
    amount: TokenAmount,
    #[command(flatten)]
    subnet: SubnetArgs,
}

#[derive(Clone, Debug, Args)]
struct TransferArgs {
    /// Wallet private key (ECDSA, secp256k1) for signing transactions.
    #[arg(short, long, env = "RECALL_PRIVATE_KEY", value_parser = parse_secret_key, hide_env_values = true)]
    private_key: SecretKey,
    /// The recipient account address.
    #[arg(long, value_parser = parse_address)]
    to: Address,
    /// The amount to transfer in FIL.
    #[arg(value_parser = parse_token_amount)]
    amount: TokenAmount,
    #[command(flatten)]
    subnet: SubnetArgs,
}

#[derive(Clone, Debug, Args)]
struct SetSponsorArgs {
    /// Wallet private key (ECDSA, secp256k1) for signing transactions.
    #[arg(short, long, env = "RECALL_PRIVATE_KEY", value_parser = parse_secret_key, hide_env_values = true)]
    private_key: SecretKey,
    /// Credit sponsor address.
    #[arg(value_parser = parse_address)]
    sponsor: Address,
    /// Broadcast mode for the transaction.
    #[arg(short, long, value_enum, env = "RECALL_BROADCAST_MODE", default_value_t = BroadcastMode::Commit)]
    broadcast_mode: BroadcastMode,
    #[command(flatten)]
    tx_args: TxArgs,
}

#[derive(Clone, Debug, Args)]
struct UnsetSponsorArgs {
    /// Wallet private key (ECDSA, secp256k1) for signing transactions.
    #[arg(short, long, env = "RECALL_PRIVATE_KEY", value_parser = parse_secret_key, hide_env_values = true)]
    private_key: SecretKey,
    /// Broadcast mode for the transaction.
    #[arg(short, long, value_enum, env = "RECALL_BROADCAST_MODE", default_value_t = BroadcastMode::Commit)]
    broadcast_mode: BroadcastMode,
    #[command(flatten)]
    tx_args: TxArgs,
}

#[derive(Clone, Debug, Args)]
pub struct SetStatusArgs {
    /// Wallet private key (ECDSA, secp256k1) for signing transactions.
    #[arg(short, long, env = "RECALL_PRIVATE_KEY", value_parser = parse_secret_key, hide_env_values = true)]
    private_key: SecretKey,
    /// Account address for which the status is being set.
    #[arg(long, value_parser = parse_address)]
    address: Address,
    /// TTL status to set.
    status: AccountStatus,
    /// Broadcast mode for the transaction.
    #[arg(short, long, value_enum, env = "RECALL_BROADCAST_MODE", default_value_t = BroadcastMode::Commit)]
    broadcast_mode: BroadcastMode,
    #[command(flatten)]
    tx_args: TxArgs,
}

/// The status of an account.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
enum AccountStatus {
    /// Default TTL.
    Default,
    /// Reduced TTL.
    Reduced,
    /// Extended TTL.
    Extended,
}

impl AccountStatus {
    pub fn get(&self) -> SdkAccountStatus {
        match self {
            AccountStatus::Default => SdkAccountStatus::Default,
            AccountStatus::Reduced => SdkAccountStatus::Reduced,
            AccountStatus::Extended => SdkAccountStatus::Extended,
        }
    }
}

/// Account commands handler.
pub async fn handle_account(
    cfg: NetworkConfig,
    args: &AccountArgs,
    verbosity: usize,
) -> anyhow::Result<()> {
    let provider =
        JsonRpcProvider::new_http(cfg.rpc_url.clone(), cfg.subnet_id.chain_id(), None, None)?;

    match &args.command {
        AccountCommands::Create => {
            let sk = random_secretkey();
            let pk = sk.public_key().serialize();
            let address = Address::from(EthAddress::new_secp256k1(&pk)?);
            let eth_address = get_eth_address(address)?;
            let sk_hex = hex::encode(sk.serialize());

            let mut json = json!({"private_key": sk_hex, "address": eth_address});
            if verbosity > 0 {
                if let Value::Object(ref mut obj) = json {
                    obj.insert(
                        "fvm_address".to_string(),
                        Value::String(address.to_string()),
                    );
                }
            }

            print_json(&json)
        }
        AccountCommands::Info(args) => {
            let address = get_address(args.address.clone(), &cfg.subnet_id)?;
            let eth_address = get_eth_address(address)?;
            let sequence =
                Account::sequence(&provider, &Void::new(address), args.address.height).await?;
            let account_balance = Account::balance(
                &Void::new(address),
                get_subnet_config(&cfg, args.subnet.clone())?,
            )
            .await?;

            let credit_balance = Credits::balance(&provider, address, args.address.height)
                .await
                .unwrap_or_else(|e| {
                    if e.to_string().contains("actor not found") {
                        Balance::default()
                    } else {
                        panic!("{:?}", e)
                    }
                });

            let mut json = json!({"address": eth_address, "sequence": sequence, "balance": account_balance.to_string(), "credit": &json!(credit_balance)});
            if verbosity > 0 {
                if let Value::Object(ref mut obj) = json {
                    obj.insert(
                        "fvm_address".to_string(),
                        Value::String(address.to_string()),
                    );
                }
            }

            match cfg.parent_network_config {
                Some(parent) => {
                    let parent_balance = Account::supply_source_balance(
                        &Void::new(address),
                        get_parent_subnet_config(&cfg.subnet_id, parent, args.subnet.clone())?,
                    )
                    .await?;

                    if let Value::Object(ref mut obj) = json {
                        obj.insert(
                            "parent_balance".to_string(),
                            Value::String(parent_balance.to_string()),
                        );
                    }

                    print_json(&json)
                }
                None => print_json(&json),
            }
        }
        AccountCommands::Deposit(args) => {
            let parent = cfg
                .parent_network_config
                .ok_or(anyhow!("address {} does not have parent", &cfg.subnet_id))?;
            let config = get_parent_subnet_config(&cfg.subnet_id, parent, args.subnet.clone())?;

            let signer = Wallet::new_secp256k1(
                args.private_key.clone(),
                AccountKind::Ethereum,
                cfg.subnet_id.parent()?, // Signer must target the parent subnet
            )?;

            let tx = Account::deposit(
                &signer,
                args.to.unwrap_or(signer.address()),
                config,
                cfg.subnet_id,
                args.amount.clone(),
            )
            .await?;

            print_json(&tx)
        }
        AccountCommands::Withdraw(args) => {
            let config = get_subnet_config(&cfg, args.subnet.clone())?;

            let signer = Wallet::new_secp256k1(
                args.private_key.clone(),
                AccountKind::Ethereum,
                cfg.subnet_id,
            )?;

            let tx = Account::withdraw(
                &signer,
                args.to.unwrap_or(signer.address()),
                config,
                args.amount.clone(),
            )
            .await?;

            print_json(&tx)
        }
        AccountCommands::Transfer(args) => {
            let config = get_subnet_config(&cfg, args.subnet.clone())?;

            let signer = Wallet::new_secp256k1(
                args.private_key.clone(),
                AccountKind::Ethereum,
                cfg.subnet_id,
            )?;

            let tx = Account::transfer(&signer, args.to, config, args.amount.clone()).await?;

            print_json(&tx)
        }
        AccountCommands::Sponsor(cmd) => match cmd {
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

                let tx = Account::set_sponsor(
                    &provider,
                    &mut signer,
                    Some(args.sponsor),
                    SetSponsorOptions {
                        broadcast_mode,
                        gas_params,
                    },
                )
                .await?;

                print_tx_json(&tx)
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

                let tx = Account::set_sponsor(
                    &provider,
                    &mut signer,
                    None,
                    SetSponsorOptions {
                        broadcast_mode,
                        gas_params,
                    },
                )
                .await?;

                print_tx_json(&tx)
            }
        },
        AccountCommands::Credit(args) => handle_credit(cfg, args).await,
        AccountCommands::SetStatus(args) => {
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
            let tx = Account::set_status(
                &provider,
                &mut signer,
                args.address,
                SetStatusOptions {
                    status: args.status.get(),
                    broadcast_mode,
                    gas_params,
                },
            )
            .await?;

            print_tx_json(&tx)
        }
    }
}

/// Returns the subnet configuration from args.
fn get_subnet_config(cfg: &NetworkConfig, args: SubnetArgs) -> anyhow::Result<EVMSubnet> {
    Ok(EVMSubnet {
        id: cfg.subnet_id.clone(),
        provider_http: args.evm_rpc_url.unwrap_or(cfg.evm_rpc_url.clone()),
        provider_timeout: Some(args.evm_rpc_timeout),
        auth_token: args.evm_rpc_auth_token,
        registry_addr: args.evm_registry.unwrap_or(cfg.evm_registry_address),
        gateway_addr: args.evm_gateway.unwrap_or(cfg.evm_gateway_address),
        supply_source: None, // supply source is not used in child subnet
    })
}

/// Returns the parent subnet configuration from args.
fn get_parent_subnet_config(
    subnet_id: &SubnetID,
    parent: ParentNetworkConfig,
    args: SubnetArgs,
) -> anyhow::Result<EVMSubnet> {
    Ok(EVMSubnet {
        id: subnet_id.parent()?,
        provider_http: args.evm_rpc_url.unwrap_or(parent.evm_rpc_url),
        provider_timeout: Some(args.evm_rpc_timeout),
        auth_token: args.evm_rpc_auth_token,
        registry_addr: args.evm_registry.unwrap_or(parent.evm_registry_address),
        gateway_addr: args.evm_gateway.unwrap_or(parent.evm_gateway_address),
        supply_source: Some(
            args.evm_supply_source
                .unwrap_or(parent.evm_supply_source_address),
        ),
    })
}
