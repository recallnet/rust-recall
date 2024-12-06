// Copyright 2024 Hoku Contributors
// SPDX-License-Identifier: Apache-2.0, MIT

use std::collections::HashMap;
use std::env;

use anyhow::anyhow;
use fendermint_actor_machine::WriteAccess;
use rand::{thread_rng, Rng};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::time::{sleep, Duration};

use hoku_provider::json_rpc::JsonRpcProvider;
use hoku_sdk::machine::bucket::{AddOptions, GetOptions, QueryOptions};
use hoku_sdk::{
    machine::{bucket::Bucket, Machine},
    network::Network,
};
use hoku_signer::{key::parse_secret_key, AccountKind, Wallet};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        return Err(anyhow!("Usage: [private key]"));
    }

    let pk_kex = &args[2];
    let pk = parse_secret_key(pk_kex)?;

    // Use testnet network defaults
    let cfg = Network::Testnet.get_config();

    // Setup network provider
    let provider = JsonRpcProvider::new_http(cfg.rpc_url, None, Some(cfg.object_api_url))?;

    // Setup local wallet using private key from arg
    let mut signer = Wallet::new_secp256k1(pk, AccountKind::Ethereum, cfg.subnet_id)?;
    signer.init_sequence(&provider).await?;

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

    // Create a temp file to add
    let mut file = async_tempfile::TempFile::new().await?;
    let mut rng = thread_rng();
    let mut random_data = vec![0; 1024 * 1024]; // 1 MiB
    rng.fill(&mut random_data[..]);
    file.write_all(&random_data).await?;
    file.flush().await?;

    // Add a file to the bucket
    let key = "foo/my_file";
    let mut metadata = HashMap::new();
    metadata.insert("foo".to_string(), "bar".to_string());
    let options = AddOptions {
        overwrite: true,
        metadata,
        ..Default::default()
    };
    let tx = machine
        .add_from_path(&provider, &mut signer, key, file.file_path(), options)
        .await?;
    println!(
        "Added 1MiB file to bucket {} with key {}",
        machine.address(),
        key,
    );
    println!("Transaction hash: 0x{}", tx.hash);

    // Wait some time for the network to resolve the object
    sleep(Duration::from_secs(2)).await;

    // Query for the object
    let options = QueryOptions {
        prefix: "foo/".into(),
        ..Default::default()
    };
    tokio::time::sleep(Duration::from_secs(2)).await;
    let list = machine.query(&provider, options).await?;
    for (key_bytes, object) in list.objects {
        let key = core::str::from_utf8(&key_bytes).unwrap_or_default();
        println!("Query result for key {}: {}", key, object.hash);
    }

    // Download the actual object at `foo/my_file`
    let obj_file = async_tempfile::TempFile::new().await?;
    let obj_path = obj_file.file_path().to_owned();
    println!("Downloading object to {}", obj_path.display());
    let options = GetOptions {
        range: Some("0-99".to_string()), // Get the first 100 bytes
        ..Default::default()
    };
    {
        let open_file = obj_file.open_rw().await?;
        machine.get(&provider, &key, open_file, options).await?;
    }
    // Read the first 10 bytes of your downloaded 100 bytes
    let mut read_file = tokio::fs::File::open(&obj_path).await?;
    let mut contents = vec![0; 10];
    read_file.read(&mut contents).await?;
    println!("Successfully read first 10 bytes of {}", obj_path.display());

    // Now, delete the object
    let tx = machine
        .delete(&provider, &mut signer, &key, Default::default())
        .await?;
    println!("Deleted object with key {} at tx 0x{}", key, tx.hash);

    Ok(())
}
