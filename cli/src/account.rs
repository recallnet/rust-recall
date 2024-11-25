// Copyright 2024 Hoku Contributors
// SPDX-License-Identifier: Apache-2.0, MIT

use std::time::Duration;

use anyhow::anyhow;
use clap::{Args, Subcommand};
use fendermint_crypto::SecretKey;
use fendermint_vm_actor_interface::eam::EthAddress;
use fvm_shared::{address::Address, econ::TokenAmount};
use reqwest::Url;
use serde_json::json;

use hoku_provider::{
    json_rpc::JsonRpcProvider,
    util::{get_delegated_address, parse_address, parse_token_amount},
};
use hoku_sdk::{
    account::Account,
    ipc::subnet::EVMSubnet,
    network::{NetworkConfig, ParentNetworkConfig},
};
use hoku_signer::{
    key::parse_secret_key, key::random_secretkey, AccountKind, Signer, SubnetID, Void, Wallet,
};

use crate::{get_address, print_json, AddressArgs};

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
    #[arg(short, long, env = "HOKU_PRIVATE_KEY", value_parser = parse_secret_key)]
    private_key: SecretKey,
    /// The recipient account address. If not present, the signer address is used.
    #[arg(long, value_parser = parse_address)]
    recipient: Option<Address>,
    /// The amount to transfer in FIL.
    #[arg(value_parser = parse_token_amount)]
    amount: TokenAmount,
    #[command(flatten)]
    subnet: SubnetArgs,
}

#[derive(Clone, Debug, Args)]
struct TransferArgs {
    /// Wallet private key (ECDSA, secp256k1) for signing transactions.
    #[arg(short, long, env = "HOKU_PRIVATE_KEY", value_parser = parse_secret_key)]
    private_key: SecretKey,
    /// The recipient account address.
    #[arg(long, value_parser = parse_address)]
    recipient: Address,
    /// The amount to transfer in FIL.
    #[arg(value_parser = parse_token_amount)]
    amount: TokenAmount,
    #[command(flatten)]
    subnet: SubnetArgs,
}

/// Account commands handler.
pub async fn handle_account(cfg: NetworkConfig, args: &AccountArgs) -> anyhow::Result<()> {
    let provider = JsonRpcProvider::new_http(cfg.rpc_url.clone(), None, None)?;

    match &args.command {
        AccountCommands::Create => {
            let sk = random_secretkey();
            let pk = sk.public_key().serialize();
            let address = Address::from(EthAddress::new_secp256k1(&pk)?);
            let eth_address = get_delegated_address(address)?;
            let sk_hex = hex::encode(sk.serialize());

            print_json(
                &json!({"private_key": sk_hex, "address": eth_address, "fvm_address": address.to_string()}),
            )
        }
        AccountCommands::Info(args) => {
            let address = get_address(args.address.clone(), &cfg.subnet_id)?;
            let eth_address = get_delegated_address(address)?;
            let sequence =
                Account::sequence(&provider, &Void::new(address), args.address.height).await?;
            let balance = Account::balance(
                &Void::new(address),
                get_subnet_config(&cfg, args.subnet.clone())?,
            )
            .await?;
            match cfg.parent_network_config {
                Some(parent) => {
                    let parent_balance = Account::balance(
                        &Void::new(address),
                        get_parent_subnet_config(&cfg.subnet_id, parent, args.subnet.clone())?,
                    )
                    .await?;

                    print_json(
                        &json!({"address": eth_address, "fvm_address": address.to_string(), "sequence": sequence, "balance": balance.to_string(), "parent_balance": parent_balance.to_string()}),
                    )
                }
                None => print_json(
                    &json!({"address": eth_address, "fvm_address": address.to_string(), "sequence": sequence, "balance": balance.to_string()}),
                ),
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
                args.recipient.unwrap_or(signer.address()),
                config,
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
                args.recipient.unwrap_or(signer.address()),
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

            let tx =
                Account::transfer(&signer, args.recipient, config, args.amount.clone()).await?;

            print_json(&tx)
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
        id: subnet_id.parent().unwrap(),
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
