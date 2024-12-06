// Copyright 2024 Hoku Contributors
// SPDX-License-Identifier: Apache-2.0, MIT

use std::collections::HashMap;
use std::sync::Arc;

use fendermint_actor_machine::WriteAccess;
use fvm_shared::econ::TokenAmount;
use num_traits::Zero;
use rand::distributions::Alphanumeric;
use rand::{Rng, SeedableRng};
use tokio::io::AsyncWriteExt;
use tokio::sync::Barrier;

use hoku_provider::json_rpc::JsonRpcProvider;
use hoku_provider::tx::BroadcastMode;
use hoku_sdk::account::Account;
use hoku_sdk::credits::Credits;
use hoku_sdk::machine::bucket::AddOptions;
use hoku_sdk::{
    machine::{bucket::Bucket, Machine},
    network::Network,
};
use hoku_signer::{key::parse_secret_key, AccountKind, Signer, Wallet};

const ADMIN_PRIVATE_KEY: &str = "1c323d494d1d069fe4c891350a1ec691c4216c17418a0cb3c7533b143bd2b812";
const USER_PRIVATE_KEYS: [&str; 10] = [
    "ac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80",
    "59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d",
    "5de4111afa1a4b94908f83103eb1f1706367c2e68ca870fc3fb9a804cdab365a",
    "7c852118294e51e653712a81e05800f419141751be58f605c371e15141b007a6",
    "47e179ec197488593b187f80a00eb0da91f1b9d0b13f8733639f19c30a34926a",
    "8b3a350cf5c34c9194ca85829a2df0ec3153be0318b5e2d3348e872092edffba",
    "92db14e403b83dfe3df233f83dfa3a0d7096f21ca9b0d6d6b8d88b2b4ec1564e",
    "4bbbf85ce3377467afe5d46f804f221813b2bb87f24d81f60f1fcdbf7cbf4356",
    "dbda1821b80551c9d65939329250298aa3472ba22feea921c0cf5d620ea67b97",
    "2a871d0798f97d79848a013d4936a73bf4cc922c825d33c1cf7073dff6d409c6",
];

const REQUESTS_PER_USER: u32 = 500;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Use devnet network defaults
    let cfg = Network::Devnet.get_config();

    // Setup network provider
    let provider =
        JsonRpcProvider::new_http(cfg.rpc_url.clone(), None, Some(cfg.object_api_url.clone()))?;

    // Setup admin wallet
    let pk = parse_secret_key(&ADMIN_PRIVATE_KEY)?;
    let subnet_id = cfg.subnet_id.clone();
    let mut admin = Wallet::new_secp256k1(pk, AccountKind::Ethereum, subnet_id.clone())?;
    admin.init_sequence(&provider).await?;

    // Buy some credits
    let admin_address = admin.address();
    let amount = TokenAmount::from_whole(500);
    let tx = Credits::buy(
        &provider,
        &mut admin,
        admin_address,
        amount,
        Default::default(),
    )
    .await?;
    println!("Admin credit balance: {}", tx.data.unwrap().credit_free);
    println!("Transaction hash: 0x{}", tx.hash);

    // Create a new bucket
    let (buck, tx) = Bucket::new(
        &provider,
        &mut admin,
        None,
        WriteAccess::OnlyOwner,
        HashMap::new(),
        Default::default(),
    )
    .await?;
    println!("Created new bucket {}", buck.address());
    println!("Transaction hash: 0x{}", tx.hash);

    // Approve users
    for i in 0..USER_PRIVATE_KEYS.len() {
        let pk = parse_secret_key(&USER_PRIVATE_KEYS[i])?;
        let user = Wallet::new_secp256k1(pk, AccountKind::Ethereum, subnet_id.clone())?;
        let subnet = cfg.subnet_config().clone();

        let tx =
            Account::transfer(&admin, user.address(), subnet, TokenAmount::from_whole(1)).await?;
        println!("Created account {}", user.address());
        println!("Transaction hash: 0x{}", tx.transaction_hash);

        admin.init_sequence(&provider).await?; // bug: Ethereum methods don't bump sequence
        let tx =
            Credits::approve(&provider, &mut admin, user.address(), Default::default()).await?;
        println!("Approved {} to spend credits", user.address());
        println!("Transaction hash: 0x{}", tx.hash);
    }

    return Ok(());

    // Simulate all users adding to the same bucket
    let buck_addr = buck.address();
    let barrier = Arc::new(Barrier::new(USER_PRIVATE_KEYS.len()));
    let mut handles = vec![];
    for i in 0..USER_PRIVATE_KEYS.len() {
        let provider = provider.clone();
        let pk = parse_secret_key(&USER_PRIVATE_KEYS[i])?;
        let subnet_id = cfg.subnet_id.clone();
        let mut user = Wallet::new_secp256k1(pk, AccountKind::Ethereum, subnet_id.clone())?;
        user.init_sequence(&provider).await?;

        // Spawn a thread for each user
        let barrier = barrier.clone();
        let handle = tokio::spawn(async move {
            barrier.wait().await;

            // Set sponsor for gas fees
            let tx = Credits::set_sponsor(
                &provider,
                &mut user,
                Some(admin_address),
                Default::default(),
            )
            .await
            .unwrap();
            println!("{} set sponsor to {}", user.address(), admin_address);
            println!("Transaction hash: 0x{}", tx.hash);

            for _ in 0..REQUESTS_PER_USER {
                // Create a temp file to add
                let mut file = async_tempfile::TempFile::new().await.unwrap();
                let mut rng = rand_chacha::ChaCha12Rng::from_entropy();
                let mut random_data = vec![0; 16];
                rng.fill(&mut random_data[..]);
                file.write_all(&random_data).await.unwrap();
                file.flush().await.unwrap();

                // Add a file to the bucket
                let buck = Bucket::attach(buck_addr).await.unwrap();
                let key = rand_string(8);
                let tx = buck
                    .add_from_path(
                        &provider,
                        &mut user,
                        key.as_str(),
                        file.file_path(),
                        AddOptions {
                            broadcast_mode: BroadcastMode::Async,
                            ..Default::default()
                        },
                    )
                    .await
                    .unwrap();
                println!("Added file to bucket {} with key {}", buck_addr, key,);
                println!("Transaction hash: 0x{}", tx.hash);
            }
        });

        handles.push(handle);
    }

    // Wait for all tasks to complete
    for handle in handles {
        handle.await.expect("Task panicked");
    }

    Ok(())
}

fn rand_string(len: usize) -> String {
    let rng = rand_chacha::ChaCha12Rng::from_entropy();
    rng.sample_iter(&Alphanumeric)
        .take(len)
        .map(char::from)
        .collect()
}
