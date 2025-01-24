use std::fmt::Debug;
use more_asserts::{assert_gt, assert_lt};

mod common;
use hoku_sdk::{account::Account, ipc::subnet::EVMSubnet, network::Network};
use hoku_signer::{key::parse_secret_key, AccountKind, Signer, Wallet};
use hoku_provider::fvm_shared::econ::TokenAmount;

#[tokio::test]
async fn runner_has_token() {
    let network_config = common::get_network();
    let sk_env = common::get_runner_secret_key();

    let sk = parse_secret_key(&sk_env).unwrap();
    // Setup local wallet using private key from env
    let signer = Wallet::new_secp256k1(sk, AccountKind::Ethereum, network_config.subnet_id.parent().unwrap()).unwrap();
    // check the balance of the wallet the integration tests will be run with
    let balance = Account::balance(
        &signer,
        EVMSubnet {
            auth_token: Some("todo: set based on env".to_owned()),
            ..network_config.subnet_config()
        },
    )
    .await.unwrap();

    // TODO: These values are arbitrary, the actual localnet value is just under 5000, and the testnet and mainnet will be different
    assert_gt!(balance, TokenAmount::from_whole(10));
    assert_lt!(balance, TokenAmount::from_whole(10000));

}
