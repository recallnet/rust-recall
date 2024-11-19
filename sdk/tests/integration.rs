use std::collections::HashMap;
use std::env;
use std::io::Cursor;
use std::time::Duration;

use fendermint_actor_machine::WriteAccess;
use hoku_provider::json_rpc::JsonRpcProvider;
use hoku_sdk::machine::bucket::{AddOptions, Bucket, GetOptions};
use hoku_sdk::machine::Machine;
use hoku_sdk::network::Network;
use hoku_signer::key::parse_secret_key;
use hoku_signer::{AccountKind, Wallet};
use tokio::io::AsyncReadExt;
use tokio::time::sleep;

#[tokio::test]
async fn test_add_encrypted() -> anyhow::Result<()> {
    let pk = match env::var("PRIVATE_KEY") {
        Ok(s) => parse_secret_key(s.as_str())?,
        Err(_) => return Ok(()) // skip the test if the env is not set
    };

    // Use testnet network defaults
    let network = Network::Localnet.init();

    // Setup network provider
    let provider =
        JsonRpcProvider::new_http(network.rpc_url()?, None, Some(network.object_api_url()?))?;

    // Setup local wallet using private key from arg
    let mut signer = Wallet::new_secp256k1(pk, AccountKind::Ethereum, network.subnet_id()?)?;
    signer.init_sequence(&provider).await?;

    // Create a new bucket
    let (machine, _) = Bucket::new(
        &provider,
        &mut signer,
        None,
        WriteAccess::OnlyOwner,
        HashMap::new(),
        Default::default(),
    )
    .await?;

    let key = "foo/my_file";
    let plaintext = "a".repeat(66000);

    let encryption_key = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";
    let options = AddOptions {
        overwrite: true,
        enc_c: Some(encryption_key.to_string()),
        ..Default::default()
    };

    let _ = machine
        .add_reader(
            &provider,
            &mut signer,
            key,
            Cursor::new(plaintext.clone()),
            options,
        )
        .await?;

    sleep(Duration::from_secs(5)).await;

    let options = GetOptions {
        enc_c: Some(encryption_key.to_string()),
        ..Default::default()
    };

    let (writer, mut reader) = tokio::io::duplex(4096);

    let provider = provider.clone();
    tokio::spawn(async move {
        machine.get(&provider, key, writer, options).await.unwrap();
    });

    let mut decrypted_content = Vec::new();
    reader.read_to_end(&mut decrypted_content).await.unwrap();

    assert_eq!(plaintext.len(), decrypted_content.len());
    assert_eq!(plaintext, std::str::from_utf8(&decrypted_content).unwrap());

    Ok(())
}

#[tokio::test]
async fn test_add_encrypted_range() -> anyhow::Result<()> {
    let pk = match env::var("PRIVATE_KEY") {
        Ok(s) => parse_secret_key(s.as_str())?,
        Err(_) => return Ok(()) // skip the test if the env is not set
    };

    // Use testnet network defaults
    let network = Network::Localnet.init();

    // Setup network provider
    let provider =
        JsonRpcProvider::new_http(network.rpc_url()?, None, Some(network.object_api_url()?))?;

    // Setup local wallet using private key from arg
    let mut signer = Wallet::new_secp256k1(pk, AccountKind::Ethereum, network.subnet_id()?)?;
    signer.init_sequence(&provider).await?;

    // Create a new bucket
    let (machine, _) = Bucket::new(
        &provider,
        &mut signer,
        None,
        WriteAccess::OnlyOwner,
        HashMap::new(),
        Default::default(),
    )
    .await?;

    let key = "foo/my_file";
    let plaintext = "abcde".repeat(40000);

    let encryption_key = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";
    let options = AddOptions {
        overwrite: true,
        enc_c: Some(encryption_key.to_string()),
        ..Default::default()
    };

    let _ = machine
        .add_reader(
            &provider,
            &mut signer,
            key,
            Cursor::new(plaintext.clone()),
            options,
        )
        .await?;

    sleep(Duration::from_secs(5)).await;

    struct TestCase {
        range: String,
        exp_content: String,
    }
    let test_cases = vec![
        TestCase {
            range: "bytes=196605-196610".into(),
            exp_content: "abcdea".into(),
        },
        TestCase {
            range: "bytes=65533-65540".into(),
            exp_content: "deabcdea".into(),
        },
        TestCase {
            range: "bytes=-1".into(),
            exp_content: "e".into(),
        },
        TestCase {
            range: "bytes=-5".into(),
            exp_content: "abcde".into(),
        },
        TestCase {
            range: "bytes=199997-".into(),
            exp_content: "cde".into(),
        },
        TestCase {
            range: "bytes=30000-89999".into(),
            exp_content: "abcde".repeat((89999 - 30000 + 1) / 5),
        },
    ];

    for tc in test_cases {
        let options = GetOptions {
            enc_c: Some(encryption_key.to_string()),
            range: Some(tc.range),
            ..Default::default()
        };

        let (writer, mut reader) = tokio::io::duplex(4096);

        let provider = provider.clone();
        let address = machine.address();
        tokio::spawn(async move {
            let machine = Bucket::attach(address).await.unwrap();
            machine.get(&provider, key, writer, options).await.unwrap();
        });

        let mut decrypted_content = Vec::new();
        reader.read_to_end(&mut decrypted_content).await.unwrap();

        assert_eq!(
            tc.exp_content,
            std::str::from_utf8(&decrypted_content).unwrap()
        );
    }

    Ok(())
}
