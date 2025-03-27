// Copyright 2025 Recall Contributors
// SPDX-License-Identifier: Apache-2.0, MIT

use recall_sdk::network::{self, NetworkConfig};
use std::{env, fs, path::Path};

const DEFAULT_TEST_TARGET_NETWORK_CONFIG_PATH: &str = "~/.config/recall/networks.toml";
const DEFAULT_TEST_TARGET_NETWORK: &str = "localnet";

pub fn get_network_config() -> NetworkConfig {
    let network_config_path = env::var("TEST_TARGET_NETWORK_CONFIG")
        .unwrap_or_else(|_| DEFAULT_TEST_TARGET_NETWORK_CONFIG_PATH.to_string());
    let network_config_path = shellexpand::full(network_config_path.as_str()).unwrap();
    let network_config_path = Path::new(network_config_path.as_ref());
    let mut specs = if !network_config_path.exists() {
        network::default_networks()
    } else {
        let file_content = fs::read_to_string(network_config_path).unwrap();
        toml::from_str(&file_content).unwrap()
    };
    let network =
        env::var("TEST_TARGET_NETWORK").unwrap_or_else(|_| DEFAULT_TEST_TARGET_NETWORK.to_string());
    specs
        .remove(&network)
        .unwrap()
        .into_network_config()
        .unwrap()
}

pub fn get_runner_secret_key() -> String {
    match env::var("RECALL_PRIVATE_KEY") {
        Ok(sk) => sk,
        Err(e) => panic!("cannot get secret key for test runner {}", e),
    }
}

pub fn get_runner_auth_token() -> String {
    env::var("RECALL_AUTH_TOKEN").unwrap_or_default()
}
