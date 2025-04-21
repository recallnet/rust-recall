// Copyright 2025 Recall Contributors
// SPDX-License-Identifier: Apache-2.0, MIT
mod account;
mod bucket;

#[cfg(test)]
pub mod test_utils {
    use recall_sdk::network::{self, NetworkConfig};
    use std::{env, fs, path::Path};

    const DEFAULT_TEST_TARGET_NETWORK_CONFIG_PATH: &str = "~/.config/recall/networks.toml";
    const DEFAULT_TEST_TARGET_NETWORK: &str = "localnet";

    // Map of test accounts and private keys. The first two Anvil test accounts are intentionally excluded below since
    // they are used to submit validator IPC transactions in the 2-node localnet setup used for testing. Using those
    // accounts in tests can lead to nonce clashing issues and cause unexpected failures.
    const DEFAULT_TEST_ACCOUNTS: [(&str, &str); 8] = [
        (
            "0x3C44CdDdB6a900fa2b585dd299e03d12FA4293BC",
            "0x5de4111afa1a4b94908f83103eb1f1706367c2e68ca870fc3fb9a804cdab365a",
        ),
        (
            "0x90F79bf6EB2c4f870365E785982E1f101E93b906",
            "0x7c852118294e51e653712a81e05800f419141751be58f605c371e15141b007a6",
        ),
        (
            "0x15d34AAf54267DB7D7c367839AAf71A00a2C6A65",
            "0x47e179ec197488593b187f80a00eb0da91f1b9d0b13f8733639f19c30a34926a",
        ),
        (
            "0x9965507D1a55bcC2695C58ba16FB37d819B0A4dc",
            "0x8b3a350cf5c34c9194ca85829a2df0ec3153be0318b5e2d3348e872092edffba",
        ),
        (
            "0x976EA74026E726554dB657fA54763abd0C3a0aa9",
            "0x92db14e403b83dfe3df233f83dfa3a0d7096f21ca9b0d6d6b8d88b2b4ec1564e",
        ),
        (
            "0x14dC79964da2C08b23698B3D3cc7Ca32193d9955",
            "0x4bbbf85ce3377467afe5d46f804f221813b2bb87f24d81f60f1fcdbf7cbf4356",
        ),
        (
            "0x23618e81E3f5cdF7f54C3d65f7FBc0aBf5B21E8f",
            "0xdbda1821b80551c9d65939329250298aa3472ba22feea921c0cf5d620ea67b97",
        ),
        (
            "0xa0Ee7A142d267C1f36714E4a8F75612F20a79720",
            "0x2a871d0798f97d79848a013d4936a73bf4cc922c825d33c1cf7073dff6d409c6",
        ),
    ];

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
            env::var("RECALL_NETWORK").unwrap_or_else(|_| DEFAULT_TEST_TARGET_NETWORK.to_string());
        specs
            .remove(&network)
            .unwrap()
            .into_network_config()
            .unwrap()
    }

    pub fn get_runner_secret_key() -> String {
        if let Ok(pk) = env::var("RECALL_PRIVATE_KEY") {
            if !pk.is_empty() {
                return pk;
            }
        }

        // Return a random private key from the test accounts
        let mut rng = rand::thread_rng();
        let random_index = rand::Rng::gen_range(&mut rng, 0..DEFAULT_TEST_ACCOUNTS.len());
        DEFAULT_TEST_ACCOUNTS[random_index].1.to_string()
    }

    pub fn get_runner_auth_token() -> String {
        env::var("RECALL_AUTH_TOKEN").unwrap_or_default()
    }
}
