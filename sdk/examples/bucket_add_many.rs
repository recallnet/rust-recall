// Copyright 2024 Hoku Contributors
// SPDX-License-Identifier: Apache-2.0, MIT

use std::collections::HashMap;
use std::env;

use anyhow::anyhow;
use fendermint_actor_machine::WriteAccess;
use fvm_shared::econ::TokenAmount;
use rand::{thread_rng, Rng};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::time::{sleep, Duration};

use hoku_provider::json_rpc::JsonRpcProvider;
use hoku_provider::tx::BroadcastMode;
use hoku_sdk::account::Account;
use hoku_sdk::credits::{BuyOptions, Credits};
use hoku_sdk::machine::bucket::{AddOptions, GetOptions, QueryOptions};
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

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Use devnet network defaults
    let cfg = Network::Devnet.get_config();

    // Setup network provider
    let provider =
        JsonRpcProvider::new_http(cfg.rpc_url.clone(), None, Some(cfg.object_api_url.clone()))?;

    // Setup local wallet using private key from arg
    let pk = parse_secret_key(&ADMIN_PRIVATE_KEY)?;
    let subnet_id = cfg.subnet_id.clone();
    let mut signer = Wallet::new_secp256k1(pk, AccountKind::Ethereum, subnet_id.clone())?;
    signer.init_sequence(&provider).await?;
    let admin = signer.address();

    // Buy some credits
    // let amount = TokenAmount::from_whole(500);
    // let tx = Credits::buy(&provider, &mut signer, admin, amount, Default::default()).await?;
    // println!("Admin credit balance: {}", tx.data.unwrap().credit_free);
    // println!("Transaction hash: 0x{}", tx.hash);

    // Create a new bucket
    let (machine, tx) = Bucket::new(
        &provider,
        &mut signer,
        None,
        WriteAccess::OnlyOwner,
        HashMap::new(),
        Default::default(),
    )
    .await?;
    println!("Created new bucket {}", machine.address());
    println!("Transaction hash: 0x{}", tx.hash);

    // Approve users
    for i in 0..USER_PRIVATE_KEYS.len() {
        let pk = parse_secret_key(&USER_PRIVATE_KEYS[i])?;
        let to = Wallet::new_secp256k1(pk, AccountKind::Ethereum, subnet_id.clone())?;
        // Remove the transfer after virtual gas is merged, which fixes this
        let subnet = cfg.subnet_config().clone();
        Account::transfer(&signer, to.address(), subnet, TokenAmount::from_whole(1)).await?;
        signer.init_sequence(&provider).await?;
        let tx = Credits::approve(
            &provider,
            &mut signer,
            admin,
            to.address(),
            Default::default(),
        )
        .await?;
        println!("Approved {} to spend credits", to.address());
        println!("Transaction hash: 0x{}", tx.hash);
    }

    // // Create a temp file to add
    // let mut file = async_tempfile::TempFile::new().await?;
    // let mut rng = thread_rng();
    // let mut random_data = vec![0; 1024 * 1024]; // 1 MiB
    // rng.fill(&mut random_data[..]);
    // file.write_all(&random_data).await?;
    // file.flush().await?;
    //
    // // Add a file to the bucket
    // let key = "foo/my_file";
    // let mut metadata = HashMap::new();
    // metadata.insert("foo".to_string(), "bar".to_string());
    // let options = AddOptions {
    //     overwrite: true,
    //     metadata,
    //     ..Default::default()
    // };
    // let tx = machine
    //     .add_from_path(&provider, &mut signer, key, file.file_path(), options)
    //     .await?;
    // println!(
    //     "Added 1MiB file to bucket {} with key {}",
    //     machine.address(),
    //     key,
    // );
    // println!("Transaction hash: 0x{}", tx.hash);
    //
    // // Wait some time for the network to resolve the object
    // sleep(Duration::from_secs(2)).await;
    //
    // // Query for the object
    // let options = QueryOptions {
    //     prefix: "foo/".into(),
    //     ..Default::default()
    // };
    // tokio::time::sleep(Duration::from_secs(2)).await;
    // let list = machine.query(&provider, options).await?;
    // for (key_bytes, object) in list.objects {
    //     let key = core::str::from_utf8(&key_bytes).unwrap_or_default();
    //     println!("Query result for key {}: {}", key, object.hash);
    // }
    //
    // // Download the actual object at `foo/my_file`
    // let obj_file = async_tempfile::TempFile::new().await?;
    // let obj_path = obj_file.file_path().to_owned();
    // println!("Downloading object to {}", obj_path.display());
    // let options = GetOptions {
    //     range: Some("0-99".to_string()), // Get the first 100 bytes
    //     ..Default::default()
    // };
    // {
    //     let open_file = obj_file.open_rw().await?;
    //     machine.get(&provider, &key, open_file, options).await?;
    // }
    // // Read the first 10 bytes of your downloaded 100 bytes
    // let mut read_file = tokio::fs::File::open(&obj_path).await?;
    // let mut contents = vec![0; 10];
    // read_file.read(&mut contents).await?;
    // println!("Successfully read first 10 bytes of {}", obj_path.display());
    //
    // // Now, delete the object
    // let tx = machine
    //     .delete(&provider, &mut signer, &key, Default::default())
    //     .await?;
    // println!("Deleted object with key {} at tx 0x{}", key, tx.hash);

    Ok(())
}
