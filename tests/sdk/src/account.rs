// Copyright 2025 Recall Contributors
// SPDX-License-Identifier: Apache-2.0, MIT
#[cfg(test)]
mod test_utils {
    use anyhow::anyhow;
    use more_asserts::{assert_gt, assert_lt};
    use recall_provider::fvm_shared::econ::TokenAmount;
    use recall_sdk::{account::Account, ipc::subnet::EVMSubnet};
    use recall_signer::{key::parse_secret_key, AccountKind, Signer, Wallet};

    use crate::test_utils;

    #[tokio::test]
    #[ignore]
    async fn runner_has_token() {
        let network_config = test_utils::get_network_config();
        let sk_env = test_utils::get_runner_secret_key();
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
                auth_token: Some(test_utils::get_runner_auth_token()),
                ..network_config.subnet_config()
            },
        )
        .await
        .unwrap();

        // TODO: These values are arbitrary, the localnet value is just under 5000, and the testnet and mainnet will depend on the runner wallet
        assert_gt!(balance, TokenAmount::from_whole(1));
        assert_lt!(balance, TokenAmount::from_whole(10000));
    }

    #[tokio::test]
    async fn can_deposit_into_subnet() {
        let network_config = test_utils::get_network_config();
        let from_sk = test_utils::get_runner_secret_key();
        let from_sk = parse_secret_key(&from_sk).unwrap();
        let from_signer = Wallet::new_secp256k1(
            from_sk,
            AccountKind::Ethereum,
            network_config.subnet_id.parent().unwrap(),
        )
        .unwrap();
        let to_sk = test_utils::get_runner_secret_key();
        let to_sk = parse_secret_key(&to_sk).unwrap();
        let subnet_id = network_config.subnet_id.clone();
        let to_signer =
            Wallet::new_secp256k1(to_sk, AccountKind::Ethereum, subnet_id.clone()).unwrap();

        // Deposit some funds into the subnet
        // Note: The debit account _must_ have Funds on parent
        let tx = match Account::deposit(
            &from_signer,
            to_signer.address(),
            network_config
                .parent_subnet_config()
                .ok_or(anyhow!("network does not have parent"))
                .unwrap(),
            subnet_id,
            TokenAmount::from_whole(10),
        )
        .await
        {
            Ok(txr) => txr,
            Err(e) => panic!("transaction failed {}", e),
        };

        println!(
            "Deposited 10 RECALL to {}",
            to_signer.eth_address().unwrap()
        );
        println!(
            "Transaction hash: 0x{}",
            hex::encode(tx.transaction_hash.to_fixed_bytes())
        );

        // TODO: some failures will throw, but we should assert that deposit worked too
    }
}
