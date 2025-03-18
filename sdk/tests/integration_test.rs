// Copyright 2025 Recall Contributors
// SPDX-License-Identifier: Apache-2.0, MIT

use std::collections::HashMap;

use anyhow::anyhow;
use more_asserts::{assert_gt, assert_lt};
use rand::{thread_rng, Rng};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::time::{sleep, Duration};

use recall_provider::{fvm_shared::econ::TokenAmount, json_rpc::JsonRpcProvider};
use recall_sdk::{
    account::Account,
    ipc::subnet::EVMSubnet,
    machine::{
        bucket::{AddOptions, Bucket, GetOptions, QueryOptions},
        Machine,
    },
};
use recall_signer::{key::parse_secret_key, AccountKind, Signer, Wallet};

mod common;

// TODO: remove the ignore once we have CI setup
#[tokio::test]
#[ignore]
async fn runner_has_token() {
    let network_config = common::get_network();
    let sk_env = common::get_runner_secret_key();
    let sk = parse_secret_key(&sk_env).unwrap();
    let signer = Wallet::new_secp256k1(
        sk,
        AccountKind::Ethereum,
        network_config.subnet_id.parent().unwrap(),
    )
    .unwrap();

    // check the balance of the wallet the integration tests will be run with
    let balance = Account::balance(
        &signer,
        EVMSubnet {
            auth_token: Some(common::get_runner_auth_token()),
            ..network_config.subnet_config()
        },
    )
    .await
    .unwrap();

    // TODO: These values are arbitrary, the localnet value is just under 5000, and the testnet and mainnet will depend on the runner wallet
    assert_gt!(balance, TokenAmount::from_whole(1));
    assert_lt!(balance, TokenAmount::from_whole(10000));
}

// TODO: this test fails, but it seems like it's because account deplosit is broken...
#[tokio::test]
#[ignore]
async fn can_deposit() {
    let network_config = common::get_network();

    let sk_env = common::get_runner_secret_key();
    let sk = parse_secret_key(&sk_env).unwrap();
    let signer = Wallet::new_secp256k1(
        sk,
        AccountKind::Ethereum,
        network_config.subnet_id.parent().unwrap(),
    )
    .unwrap();

    // Deposit some funds into the subnet
    // Note: The debit account _must_ have Funds on parent
    let tx = match Account::deposit(
        &signer,
        signer.address(),
        network_config
            .parent_subnet_config()
            .ok_or(anyhow!("network does not have parent"))
            .unwrap(),
        network_config.subnet_id,
        TokenAmount::from_whole(10),
    )
    .await
    {
        Ok(txr) => txr,
        Err(e) => panic!("transaction failed {}", e),
    };

    println!("Deposited 1 RECALL to {}", signer.eth_address().unwrap());
    println!(
        "Transaction hash: 0x{}",
        hex::encode(tx.transaction_hash.to_fixed_bytes())
    );

    // TODO: some failures will throw, but we should assert that deposit worked too
}

#[tokio::test]
#[ignore]
async fn can_add_bucket() {
    let network_config = common::get_network();
    let sk_env = common::get_runner_secret_key();
    let sk = parse_secret_key(&sk_env).unwrap();
    let mut signer =
        Wallet::new_secp256k1(sk, AccountKind::Ethereum, network_config.subnet_id.clone()).unwrap();

    // Setup network provider
    let provider = JsonRpcProvider::new_http(
        network_config.rpc_url,
        network_config.subnet_id.chain_id(),
        None,
        Some(network_config.object_api_url),
    )
    .unwrap();
    // Setup wallet using private key from arg
    signer.init_sequence(&provider).await.unwrap();

    // Create a new bucket
    let (buck, _) = Bucket::new(
        &provider,
        &mut signer,
        None,
        HashMap::new(),
        Default::default(),
    )
    .await
    .unwrap();

    // Create a temp file to add
    let mut file = async_tempfile::TempFile::new().await.unwrap();
    let mut rng = thread_rng();
    let mut random_data = vec![0; 1024 * 1024]; // 1 MiB
    rng.fill(&mut random_data[..]);
    file.write_all(&random_data).await.unwrap();
    file.flush().await.unwrap();

    // Add a file to the bucket
    let key = "foo/my_file";
    let mut metadata = HashMap::new();
    metadata.insert("foo".to_string(), "bar".to_string());
    let options = AddOptions {
        overwrite: true,
        metadata,
        ..Default::default()
    };
    let from = signer.address();
    buck
        .add_from_path(&provider, &mut signer, from, key, file.file_path(), options)
        .await
        .unwrap();

    // Wait some time for the network to resolve the object
    sleep(Duration::from_secs(2)).await;

    // Query for the object
    let options = QueryOptions {
        prefix: "foo/".into(),
        ..Default::default()
    };
    sleep(Duration::from_secs(2)).await;
    let list = buck.query(&provider, options).await.unwrap();
    for (key_bytes, object) in list.objects {
        let query_key = core::str::from_utf8(&key_bytes).unwrap_or_default();

        assert_eq!(key, query_key);
        assert_eq!(object.metadata.get("foo").unwrap(), "bar");
        assert_eq!(
            object.metadata.get("content-type").unwrap(),
            "application/octet-stream"
        );
    }

    // Download the actual object at `foo/my_file`
    let obj_file = async_tempfile::TempFile::new().await.unwrap();
    let obj_path = obj_file.file_path().to_owned();

    let options = GetOptions {
        range: Some("0-99".to_string()), // Get the first 100 bytes
        ..Default::default()
    };
    let open_file = obj_file.open_rw().await.unwrap();
    buck
        .get(&provider, key, open_file, options)
        .await
        .unwrap();

    // Read the first 10 bytes of your downloaded 100 bytes
    let mut read_file = tokio::fs::File::open(&obj_path).await.unwrap();
    let mut contents = vec![0; 10];
    read_file.read_exact(&mut contents).await.unwrap();

    assert_eq!(contents, &random_data[0..10]);

    // Now, delete the object
    buck
        .delete(&provider, &mut signer, from, key, Default::default())
        .await
        .unwrap();

    // TODO: failure might throw, but need to add assertion for deleting
}
