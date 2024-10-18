// Copyright 2024 Hoku Contributors
// SPDX-License-Identifier: Apache-2.0, MIT

use std::collections::HashMap;
use std::env;

use anyhow::anyhow;
use bytes::Bytes;
use fendermint_actor_machine::WriteAccess;
use fendermint_vm_message::query::FvmQueryHeight;

use hoku_provider::json_rpc::JsonRpcProvider;
use hoku_sdk::{
    machine::{timehub::Timehub, Machine},
    network::Network,
};
use hoku_signer::{key::parse_secret_key, AccountKind, Wallet};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        return Err(anyhow!("missing hex-encoded private key"));
    }
    let pk_kex = &args[1];
    let pk = parse_secret_key(pk_kex)?;

    // Use testnet network defaults
    let network = Network::Testnet.init();

    // Setup network provider
    let provider = JsonRpcProvider::new_http(network.rpc_url()?, None, None)?;

    // Setup local wallet using private key from arg
    let mut signer = Wallet::new_secp256k1(pk, AccountKind::Ethereum, network.subnet_id()?)?;
    signer.init_sequence(&provider).await?;

    // Create a new timehub
    let (machine, tx) = Timehub::new(
        &provider,
        &mut signer,
        None,
        WriteAccess::OnlyOwner,
        HashMap::new(),
        Default::default(),
    )
    .await?;
    println!("Created new timehub {}", machine.address(),);
    println!("Transaction hash: 0x{}", tx.hash);

    // Push a value to the timehub
    let value = Bytes::from("my_value");
    let tx = machine
        .push(&provider, &mut signer, value, Default::default())
        .await?;
    println!(
        "Pushed to timehub {} with index {}",
        machine.address(),
        tx.data.unwrap().index // Safe if broadcast mode is "commit". See `PushOptions`.
    );
    println!("Transaction hash: 0x{}", tx.hash);

    // Get the value back
    let result = machine
        .leaf(&provider, 0, FvmQueryHeight::Committed)
        .await?;

    match result {
        None => {
            println!("No value at the given index!")
        }
        Some((timestamp, value)) => {
            println!(
                "index 0 timestamp: {}, value: '{}'",
                timestamp,
                std::str::from_utf8(&value)?
            );
        }
    }

    // Query for count
    let count = machine.count(&provider, FvmQueryHeight::Committed).await?;
    println!("Count: {}", count);

    // Query for the new root
    let root = machine.root(&provider, FvmQueryHeight::Committed).await?;
    println!("State root: {}", root);

    Ok(())
}
