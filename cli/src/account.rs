// Copyright 2024 ADM Contributors
// SPDX-License-Identifier: Apache-2.0, MIT

use std::time::Duration;

use clap::{Args, Subcommand};
use ethers::prelude::TransactionReceipt;
use fendermint_crypto::SecretKey;
use fendermint_vm_actor_interface::eam::EthAddress;
use fvm_shared::{address::Address, econ::TokenAmount};
use reqwest::{Client, Url};
use serde_json::json;

use adm_provider::{
    json_rpc::JsonRpcProvider,
    util::{get_delegated_address, parse_address, parse_token_amount},
};
use adm_sdk::{account::Account, ipc::subnet::EVMSubnet, network::Network as SdkNetwork};
use adm_signer::{
    key::parse_secret_key, key::random_secretkey, AccountKind, Signer, SubnetID, Void, Wallet,
};

use crate::{get_address, get_rpc_url, get_subnet_id, print_json, AddressArgs, Cli};

#[derive(Clone, Debug, Args)]
pub struct AccountArgs {
    #[command(subcommand)]
    command: AccountCommands,
}

#[derive(Clone, Debug, Subcommand)]
enum AccountCommands {
    /// Create a new account from a random seed.
    Create,
    /// Register a new account on a subnet.
    Register(RegisterArgs),
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
    #[arg(short, long, env, value_parser = parse_secret_key)]
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
    #[arg(short, long, env, value_parser = parse_secret_key)]
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
struct RegisterArgs {
    /// Wallet private key (ECDSA, secp256k1) for signing transactions.
    #[arg(short, long, env, value_parser = parse_secret_key)]
    private_key: Option<SecretKey>,
    /// Account address. The signer address is used if no address is given.
    #[arg(short, long, value_parser = parse_address)]
    address: Option<Address>,
    /// Wallet registration URL. This sends a subnet transaction from a
    /// sponsoring wallet to new accounts, covering gas fees.
    #[arg(long, env)]
    faucet_url: Option<Url>,
    #[command(flatten)]
    subnet: SubnetArgs,
}

/// Account commands handler.
pub async fn handle_account(cli: Cli, args: &AccountArgs) -> anyhow::Result<()> {
    let provider = JsonRpcProvider::new_http(get_rpc_url(&cli)?, None, None)?;
    let subnet_id = get_subnet_id(&cli)?;

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
        AccountCommands::Register(args) => {
            let addr_args = AddressArgs {
                private_key: args.private_key.clone(),
                address: args.address,
                height: Default::default(),
            };
            let height = addr_args.height;
            let address = get_address(addr_args, &subnet_id)?;
            let eth_address = get_delegated_address(address)?;
            let eth_addr_str = format!("{:?}", eth_address);

            match Account::sequence(&provider, &Void::new(address), height).await {
                Ok(_) => {
                    println!("account already registered");
                    Ok(())
                }
                Err(_) => {
                    let network = cli.network.get();
                    let base_url = get_faucet_url(network, args.faucet_url.clone())?;
                    let url = base_url.join("register").unwrap();
                    let body = json!({
                        "network": network.to_string(),
                        "address": eth_addr_str
                    });
                    let req = Client::new()
                        .post(url)
                        .header("Content-Type", "application/json")
                        .body(body.to_string())
                        .send()
                        .await?;
                    let tx: TransactionReceipt = req.json().await?;

                    print_json(&tx)
                }
            }
        }
        AccountCommands::Info(args) => {
            let address = get_address(args.address.clone(), &subnet_id)?;
            let eth_address = get_delegated_address(address)?;
            let sequence =
                Account::sequence(&provider, &Void::new(address), args.address.height).await?;
            let balance = Account::balance(
                &Void::new(address),
                get_subnet_config(&cli, &subnet_id, args.subnet.clone())?,
            )
            .await?;
            let parent_balance = Account::balance(
                &Void::new(address),
                get_parent_subnet_config(&cli, &subnet_id, args.subnet.clone())?,
            )
            .await?;

            print_json(
                &json!({"address": eth_address, "fvm_address": address.to_string(), "sequence": sequence, "balance": balance.to_string(), "parent_balance": parent_balance.to_string()}),
            )
        }
        AccountCommands::Deposit(args) => {
            let config = get_parent_subnet_config(&cli, &subnet_id, args.subnet.clone())?;

            let signer = Wallet::new_secp256k1(
                args.private_key.clone(),
                AccountKind::Ethereum,
                subnet_id.parent()?, // Signer must target the parent subnet
            )?;

            let tx = Account::deposit(
                &signer,
                args.to.unwrap_or(signer.address()),
                config,
                args.amount.clone(),
            )
            .await?;

            print_json(&tx)
        }
        AccountCommands::Withdraw(args) => {
            let config = get_subnet_config(&cli, &subnet_id, args.subnet.clone())?;

            let signer =
                Wallet::new_secp256k1(args.private_key.clone(), AccountKind::Ethereum, subnet_id)?;

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
            let config = get_subnet_config(&cli, &subnet_id, args.subnet.clone())?;

            let signer =
                Wallet::new_secp256k1(args.private_key.clone(), AccountKind::Ethereum, subnet_id)?;

            let tx = Account::transfer(&signer, args.to, config, args.amount.clone()).await?;

            print_json(&tx)
        }
    }
}

/// Returns the subnet configuration from args.
fn get_subnet_config(cli: &Cli, id: &SubnetID, args: SubnetArgs) -> anyhow::Result<EVMSubnet> {
    let network = cli.network.get();
    Ok(EVMSubnet {
        id: id.clone(),
        provider_http: args.evm_rpc_url.unwrap_or(network.evm_rpc_url()?),
        provider_timeout: Some(args.evm_rpc_timeout),
        auth_token: args.evm_rpc_auth_token,
        registry_addr: args.evm_registry.unwrap_or(network.evm_registry()?),
        gateway_addr: args.evm_gateway.unwrap_or(network.evm_gateway()?),
    })
}

/// Returns the parent subnet configuration from args.
fn get_parent_subnet_config(
    cli: &Cli,
    id: &SubnetID,
    args: SubnetArgs,
) -> anyhow::Result<EVMSubnet> {
    let network = cli.network.get();
    Ok(EVMSubnet {
        id: id.clone(),
        provider_http: args.evm_rpc_url.unwrap_or(network.parent_evm_rpc_url()?),
        provider_timeout: Some(args.evm_rpc_timeout),
        auth_token: args.evm_rpc_auth_token,
        registry_addr: args.evm_registry.unwrap_or(network.parent_evm_registry()?),
        gateway_addr: args.evm_gateway.unwrap_or(network.parent_evm_gateway()?),
    })
}

/// Returns url to register subnet accounts from a sponsoring wallet. Note: only
/// `testnet` is supported.
fn get_faucet_url(network: SdkNetwork, url: Option<Url>) -> anyhow::Result<Url> {
    match url {
        Some(u) => Ok(u),
        None => network.faucet_api_url(),
    }
}
