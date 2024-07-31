// Copyright 2024 ADM Contributors
// SPDX-License-Identifier: Apache-2.0, MIT

use std::env;

use anyhow::anyhow;
use fendermint_actor_machine::WriteAccess;
use rand::{thread_rng, Rng};
use tokio::io::AsyncWriteExt;
use tokio::time::{sleep, Duration};

use adm_provider::json_rpc::JsonRpcProvider;
use adm_sdk::machine::objectstore::QueryOptions;
use adm_sdk::{
    machine::{objectstore::ObjectStore, Machine},
    network::Network,
};
use adm_signer::{key::parse_secret_key, AccountKind, Wallet};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        return Err(anyhow!("missing hex-encoded private key"));
    }
    let pk_kex = &args[1];
    let pk = parse_secret_key(pk_kex)?;

    // Use testnet network defaults
    let network = Network::Localnet.init();

    // Setup network provider
    let provider =
        JsonRpcProvider::new_http(network.rpc_url()?, None, Some(network.object_api_url()?))?;

    // Setup local wallet using private key from arg
    let mut signer = Wallet::new_secp256k1(pk, AccountKind::Ethereum, network.subnet_id()?)?;
    signer.init_sequence(&provider).await?;

    // Create a new object store
    let (machine, tx) = ObjectStore::new(
        &provider,
        &mut signer,
        WriteAccess::OnlyOwner,
        Default::default(),
    )
    .await?;
    println!("Created new object store {}", machine.address());
    println!("Transaction hash: 0x{}", tx.hash);

    // Create a temp file to add
    let mut file = async_tempfile::TempFile::new().await?;
    let mut rng = thread_rng();
    let mut random_data = vec![0; 1024 * 1024]; // 1 MiB
    rng.fill(&mut random_data[..]);
    file.write_all(&random_data).await?;
    file.flush().await?;

    // Add a file to the object store
    let key = "foo/my_file";
    let tx = machine
        .add_from_path(
            &provider,
            &mut signer,
            key,
            file.file_path(),
            Default::default(),
            network.iroh()?,
        )
        .await?;
    println!(
        "Added 1MiB file to object store {} with key {}",
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
    let list = machine.query(&provider, options).await?;
    for (key_bytes, object) in list.objects {
        let key = core::str::from_utf8(&key_bytes).unwrap_or_default();
        // `resolved` indicates the validators were able to fetch and verify the file
        let cid = cid::Cid::try_from(object.cid.0)?;
        println!(
            "Query result cid: {} (key={}; detached; resolved={})",
            cid, key, object.resolved
        );
    }

    Ok(())
}
