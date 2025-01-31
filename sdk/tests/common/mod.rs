use hoku_sdk::network::{Network, NetworkConfig};
use std::env;

#[allow(dead_code)]
pub fn setup() {
    // TODO
}

pub fn get_network() -> NetworkConfig {
    let net_name = match env::var("TEST_TARGET_NETWORK") {
        Ok(network) => network,
        Err(e) => panic!("cannot get test target network {}", e),
    };

    match net_name.as_str() {
        "localnet" => {
            Network::Localnet.init();
            Network::Localnet.get_config()
        }
        "testnet" => {
            Network::Testnet.init();
            Network::Testnet.get_config()
        }
        "mainnet" => {
            Network::Mainnet.init();
            Network::Mainnet.get_config()
        }
        _ => panic!("cannot get test target network config"),
    }
}

pub fn get_runner_secret_key() -> String {
    match env::var("HOKU_PRIVATE_KEY") {
        Ok(sk) => sk,
        Err(e) => panic!("cannot get secret key for test runner {}", e),
    }
}

pub fn get_runner_auth_token() -> String {
    env::var("HOKU_AUTH_TOKEN").unwrap_or_default()
}
