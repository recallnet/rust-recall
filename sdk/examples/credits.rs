// Copyright 2024 Hoku Contributors
// SPDX-License-Identifier: Apache-2.0, MIT

use std::env;

use anyhow::anyhow;

use hoku_provider::{
    fvm_shared::econ::TokenAmount, json_rpc::JsonRpcProvider, message::GasParams,
    query::FvmQueryHeight, tx::TxStatus,
};
use hoku_sdk::{
    account::{Account, EVMSubnet},
    credits::{ApproveOptions, BuyOptions, Credits, RevokeOptions},
    network::Network,
};
use hoku_signer::{
    key::{parse_secret_key, random_secretkey},
    AccountKind, Signer, Wallet,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        return Err(anyhow!("Usage: [private key]"));
    }

    let pk_hex = &args[1];
    let pk = parse_secret_key(pk_hex)?;

    // Use testnet network defaults
    let cfg = Network::Testnet.get_config();

    // Setup network provider
    let provider = JsonRpcProvider::new_http(cfg.rpc_url, cfg.subnet_id.chain_id(), None, None)?;

    // Setup local wallet using private key from arg
    let mut signer = Wallet::new_secp256k1(pk, AccountKind::Ethereum, cfg.subnet_id.clone())?;
    signer.init_sequence(&provider).await?;

    let signer_address = signer.address();

    // Create a second wallet to approve credits for
    let second_key = random_secretkey();
    let second_wallet =
        Wallet::new_secp256k1(second_key, AccountKind::Ethereum, cfg.subnet_id.clone())?;
    let second_address = second_wallet.address();

    println!("Main wallet address: {}", signer_address);
    println!("Second wallet address: {}", second_address);

    // Initialize the second account by sending a zero-value transaction
    let subnet = EVMSubnet {
        id: cfg.subnet_id.clone(),
        provider_http: cfg.evm_rpc_url.clone(),
        provider_timeout: None,
        auth_token: None,
        registry_addr: cfg.evm_registry_address,
        gateway_addr: cfg.evm_gateway_address,
        supply_source: None,
    };

    let tx = Account::transfer(&signer, second_address, subnet, TokenAmount::from_whole(0)).await?;
    println!(
        "Initialized second account - Transaction hash: {}",
        tx.transaction_hash
    );

    // Wait for the transfer to be confirmed
    tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;

    signer.init_sequence(&provider).await?;

    // Now we can initialize the second wallet's sequence
    let mut second_wallet = second_wallet;
    second_wallet.init_sequence(&provider).await?;

    // First, let's check the current credit balance
    let balance = Credits::balance(&provider, signer_address, FvmQueryHeight::Committed).await?;
    println!("Initial credit balance: {:?}", balance);

    // Buy credits for our account
    let amount = TokenAmount::from_whole(1); // Buy 1 token worth of credits
    let buy_options = BuyOptions {
        gas_params: GasParams {
            gas_limit: 0, // Let the system estimate gas
            ..Default::default()
        },
        ..Default::default()
    };

    let tx = Credits::buy(&provider, &mut signer, signer_address, amount, buy_options).await?;
    println!("Bought credits - Transaction hash: 0x{}", tx.hash());
    if let TxStatus::Committed(receipt) = tx.status {
        println!("Gas used: {}", receipt.gas_used.unwrap_or_default());
    }
    println!("New balance: {:?}", tx.data);

    // Approve credits for the second wallet
    let approve_options = ApproveOptions {
        credit_limit: None,  // No limit
        gas_fee_limit: None, // No gas fee limit
        ttl: None,           // No time limit
        gas_params: GasParams {
            gas_limit: 0, // Let the system estimate gas
            ..Default::default()
        },
        ..Default::default()
    };

    let tx = Credits::approve(
        &provider,
        &mut signer,
        signer_address,
        second_address,
        approve_options,
    )
    .await?;
    println!("Approved credits - Transaction hash: 0x{}", tx.hash());
    if let TxStatus::Committed(receipt) = tx.status {
        println!("Gas used: {}", receipt.gas_used.unwrap_or_default());
    }
    println!("Approval details: {:?}", tx.data);

    // Wait for the approval to be confirmed
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    // Check the second wallet's balance and approvals
    let second_balance =
        Credits::balance(&provider, second_address, FvmQueryHeight::Committed).await?;
    println!("Second wallet credit balance: {:?}", second_balance);
    println!(
        "Second wallet credit approvals: {:?}",
        second_balance.approvals_from
    );

    // Revoke credits from the second wallet
    let revoke_options = RevokeOptions {
        gas_params: GasParams {
            gas_limit: 0, // Let the system estimate gas
            ..Default::default()
        },
        ..Default::default()
    };

    let tx = Credits::revoke(
        &provider,
        &mut signer,
        signer_address,
        second_address,
        revoke_options,
    )
    .await?;
    println!("Revoked credits - Transaction hash: 0x{}", tx.hash());
    if let TxStatus::Committed(receipt) = tx.status {
        println!("Gas used: {}", receipt.gas_used.unwrap_or_default());
    }

    // Wait for revocation to be confirmed
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    // Check first wallet's balance and approvals to verify revocation
    let first_balance =
        Credits::balance(&provider, signer_address, FvmQueryHeight::Committed).await?;
    println!(
        "First wallet approvals after revocation: {:?}",
        first_balance.approvals_from
    );

    // Check final credit balances
    let final_balance =
        Credits::balance(&provider, signer_address, FvmQueryHeight::Committed).await?;
    println!("Final main wallet credit balance: {:?}", final_balance);

    let final_second_balance =
        Credits::balance(&provider, second_address, FvmQueryHeight::Committed).await?;
    println!(
        "Final second wallet credit balance: {:?}",
        final_second_balance
    );

    Ok(())
}
