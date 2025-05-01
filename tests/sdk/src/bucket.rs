// Copyright 2025 Recall Contributors
// SPDX-License-Identifier: Apache-2.0, MIT
#[cfg(test)]
mod tests {
    use rand::{thread_rng, Rng};
    use std::collections::HashMap;
    use std::time::Duration;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::time::sleep;

    use recall_provider::json_rpc::JsonRpcProvider;
    use recall_sdk::machine::{
        bucket::{AddOptions, Bucket, GetOptions, QueryOptions},
        Machine,
    };
    use recall_signer::{key::parse_secret_key, AccountKind, Wallet};

    use crate::test_utils;

    #[tokio::test]
    #[ignore]
    async fn can_add_bucket() {
        let network_config = test_utils::get_network_config();
        let sk_env = test_utils::get_runner_secret_key();
        let sk = parse_secret_key(&sk_env).unwrap();
        let mut signer =
            Wallet::new_secp256k1(sk, AccountKind::Ethereum, network_config.subnet_id.clone())
                .unwrap();

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
        let (machine, _) = Bucket::new(
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
        machine
            .add_from_path(&provider, &mut signer, key, file.file_path(), options)
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
        let list = machine.query(&provider, options).await.unwrap();
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
        machine
            .get(&provider, key, open_file, options)
            .await
            .unwrap();

        // Read the first 10 bytes of your downloaded 100 bytes
        let mut read_file = tokio::fs::File::open(&obj_path).await.unwrap();
        let mut contents = vec![0; 10];
        read_file.read_exact(&mut contents).await.unwrap();

        assert_eq!(contents, &random_data[0..10]);

        // Now, delete the object
        machine
            .delete(&provider, &mut signer, key, Default::default())
            .await
            .unwrap();

        // TODO: failure might throw, but need to add assertion for deleting
    }
}
