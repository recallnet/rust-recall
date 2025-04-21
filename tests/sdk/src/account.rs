// Copyright 2025 Recall Contributors
// SPDX-License-Identifier: Apache-2.0, MIT
#[cfg(test)]
mod tests {
    use std::ops::Sub;
    use std::time::Duration;

    use anyhow::anyhow;
    use more_asserts::{assert_ge, assert_gt};
    use tokio::time::timeout;

    use recall_provider::fvm_shared::econ::TokenAmount;
    use recall_sdk::{account::Account, ipc::subnet::EVMSubnet, network::NetworkConfig};
    use recall_signer::{key::parse_secret_key, AccountKind, Signer, Wallet};

    use crate::test_utils::{self, get_runner_auth_token};

    async fn get_account_balances(
        signer: &Wallet,
        network_config: NetworkConfig,
    ) -> (TokenAmount, TokenAmount) {
        let account_balance = Account::balance(
            signer,
            EVMSubnet {
                auth_token: Some(get_runner_auth_token()),
                ..network_config.subnet_config()
            },
        )
        .await
        .unwrap();
        let supply_source_balance = Account::supply_source_balance(
            signer,
            EVMSubnet {
                auth_token: Some(get_runner_auth_token()),
                ..network_config
                    .parent_subnet_config()
                    .ok_or(anyhow!("network does not have parent"))
                    .unwrap()
            },
        )
        .await
        .unwrap();
        (account_balance, supply_source_balance)
    }

    #[tokio::test]
    async fn can_deposit_into_subnet() {
        let network_config = test_utils::get_network_config();
        let sk = test_utils::get_runner_secret_key();
        let sk = parse_secret_key(&sk).unwrap();
        let signer = Wallet::new_secp256k1(
            sk,
            AccountKind::Ethereum,
            network_config.subnet_id.parent().unwrap(),
        )
        .unwrap();

        let (account_balance, supply_source_balance) =
            get_account_balances(&signer, network_config.clone()).await;
        // Account balance might be 0
        assert_ge!(account_balance, TokenAmount::from_whole(0));
        // Supply source balance should be greater than 0
        assert_gt!(supply_source_balance, TokenAmount::from_whole(0));

        let tokens_to_deposit = TokenAmount::from_whole(1);

        // Deposit some funds into the subnet
        Account::deposit(
            &signer,
            signer.address(),
            network_config
                .parent_subnet_config()
                .ok_or(anyhow!("network does not have parent"))
                .unwrap(),
            network_config.subnet_id.clone(),
            tokens_to_deposit.clone(),
        )
        .await
        .unwrap();

        // Wait for the balances to be updated
        assert!(
            timeout(Duration::from_secs(120), async {
                loop {
                    let (updated_account_balance, updated_supply_source_balance) =
                        get_account_balances(&signer, network_config.clone()).await;
                    if (updated_account_balance.clone().sub(&account_balance) == tokens_to_deposit)
                        && (supply_source_balance
                            .clone()
                            .sub(&updated_supply_source_balance)
                            == tokens_to_deposit)
                    {
                        println!(
                            "Account balance for {} updated from {} to {}",
                            signer.eth_address().unwrap(),
                            account_balance,
                            updated_account_balance
                        );
                        println!(
                            "Supply source balance for {} updated from {} to {}",
                            signer.eth_address().unwrap(),
                            supply_source_balance,
                            updated_supply_source_balance
                        );
                        return;
                    }
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
            })
            .await
            .is_ok(),
            "Timeout waiting for balances to update"
        );
    }
}
