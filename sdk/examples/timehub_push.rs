// Copyright 2025 Recall Contributors
// SPDX-License-Identifier: Apache-2.0, MIT

use std::env;
use std::{collections::HashMap, str::FromStr as _};

use anyhow::anyhow;
use cid::Cid;

use recall_provider::{json_rpc::JsonRpcProvider, query::FvmQueryHeight};
use recall_sdk::machine::timehub::Leaf;
use recall_sdk::{
    machine::{timehub::Timehub, Machine},
    network::Network,
};
use recall_signer::{key::parse_secret_key, AccountKind, Wallet};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        return Err(anyhow!("missing hex-encoded private key"));
    }
    let pk_kex = &args[1];
    let pk = parse_secret_key(pk_kex)?;

    // Use testnet network defaults
    let cfg = Network::Testnet.get_config();

    // Setup network provider
    let provider = JsonRpcProvider::new_http(cfg.rpc_url, cfg.subnet_id.chain_id(), None, None)?;

    // Setup local wallet using private key from arg
    let mut signer = Wallet::new_secp256k1(pk, AccountKind::Ethereum, cfg.subnet_id)?;
    signer.init_sequence(&provider).await?;

    // Create a new timehub
    let (machine, tx) = Timehub::new(
        &provider,
        &mut signer,
        None,
        HashMap::new(),
        Default::default(),
    )
    .await?;
    println!("Created new timehub {}", machine.address(),);
    println!("Transaction hash: 0x{}", tx.hash());

    // Push a value to the accumulator
    let value =
        Cid::from_str("baeabeif2afeua6dg23holphe2ecingsqr7sjo5gdbmtvekjybzspxpmaf4")?.to_bytes();
    let tx = machine
        .push(&provider, &mut signer, value.into(), Default::default())
        .await?;
    println!(
        "Pushed to timehub {} with index {}",
        machine.address(),
        tx.data.clone().unwrap().index // Safe if broadcast mode is "commit". See `PushOptions`.
    );
    println!("Transaction hash: 0x{}", tx.hash());

    // Get the value back
    let result = machine
        .leaf(&provider, 0, FvmQueryHeight::Committed)
        .await?;

    match result {
        None => {
            println!("No value at the given index!")
        }
        Some(Leaf {
            timestamp,
            witnessed,
        }) => {
            println!("index 0 timestamp: {timestamp}, value: '{witnessed}'",);
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
