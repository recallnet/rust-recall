// Copyright 2024 ADM Contributors
// SPDX-License-Identifier: Apache-2.0, MIT

use std::str::FromStr;

use anyhow::anyhow;
use fvm_shared::address::{set_current_network, Address, Network as FvmNetwork};
use tendermint_rpc::Url;

use adm_provider::util::parse_address;
use adm_signer::SubnetID;

const TESTNET_SUBNET_ID: &str = "/r314159/t410ftytqyw4taos3b2grmyq4cvhgvvth3aehutsdjvy";
const DEVNET_SUBNET_ID: &str = "test";

const TESTNET_OBJECT_API_URL: &str = "https://object-api.n1.testnet.basin.storage";
const DEVNET_OBJECT_API_URL: &str = "http://127.0.0.1:8001";

const TESTNET_RPC_URL: &str = "https://api.n1.testnet.basin.storage";
const DEVNET_RPC_URL: &str = "http://127.0.0.1:26657";

const TESTNET_EVM_RPC_URL: &str = "https://evm-api.n1.testnet.basin.storage";
const TESTNET_EVM_GATEWAY_ADDRESS: &str = "0x77aa40b105843728088c0132e43fc44348881da8";
const TESTNET_EVM_REGISTRY_ADDRESS: &str = "0x74539671a1d2f1c8f200826baba665179f53a1b7";

const TESTNET_PARENT_EVM_RPC_URL: &str = "https://api.calibration.node.glif.io/rpc/v1";
const TESTNET_PARENT_EVM_GATEWAY_ADDRESS: &str = "0x17972fF0290d8a607cd9Bd03B5c91aBe5B9E0f6E";
const TESTNET_PARENT_EVM_REGISTRY_ADDRESS: &str = "0x7b77B1e4E9341C7B563002E4F9836E352f18B348";

/// Set current network to use testnet addresses.
pub fn use_testnet_addresses() {
    set_current_network(FvmNetwork::Testnet);
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Network {
    /// Network presets for mainnet.
    Mainnet,
    /// Network presets for Calibration (default pre-mainnet).
    Testnet,
    /// Network presets for local development.
    Devnet,
}

impl Network {
    pub fn subnet(&self) -> anyhow::Result<SubnetID> {
        match self {
            Network::Mainnet => Err(anyhow!("network is pre-mainnet")),
            Network::Testnet => Ok(SubnetID::from_str(TESTNET_SUBNET_ID)?),
            Network::Devnet => Ok(SubnetID::from_str(DEVNET_SUBNET_ID)?),
        }
    }

    pub fn rpc_url(&self) -> anyhow::Result<Url> {
        match self {
            Network::Mainnet => Err(anyhow!("network is pre-mainnet")),
            Network::Testnet => Ok(Url::from_str(TESTNET_RPC_URL)?),
            Network::Devnet => Ok(Url::from_str(DEVNET_RPC_URL)?),
        }
    }

    pub fn object_api_url(&self) -> anyhow::Result<Url> {
        match self {
            Network::Mainnet => Err(anyhow!("network is pre-mainnet")),
            Network::Testnet => Ok(Url::from_str(TESTNET_OBJECT_API_URL)?),
            Network::Devnet => Ok(Url::from_str(DEVNET_OBJECT_API_URL)?),
        }
    }

    pub fn evm_rpc_url(&self) -> anyhow::Result<reqwest::Url> {
        match self {
            Network::Mainnet => Err(anyhow!("network is pre-mainnet")),
            Network::Testnet => Ok(reqwest::Url::from_str(TESTNET_EVM_RPC_URL)?),
            Network::Devnet => Err(anyhow!("network has no parent")),
        }
    }

    pub fn evm_gateway(&self) -> anyhow::Result<Address> {
        match self {
            Network::Mainnet => Err(anyhow!("network is pre-mainnet")),
            Network::Testnet => Ok(parse_address(TESTNET_EVM_GATEWAY_ADDRESS)?),
            Network::Devnet => Err(anyhow!("network has no parent")),
        }
    }

    pub fn evm_registry(&self) -> anyhow::Result<Address> {
        match self {
            Network::Mainnet => Err(anyhow!("network is pre-mainnet")),
            Network::Testnet => Ok(parse_address(TESTNET_EVM_REGISTRY_ADDRESS)?),
            Network::Devnet => Err(anyhow!("network has no parent")),
        }
    }

    pub fn parent_evm_rpc_url(&self) -> anyhow::Result<reqwest::Url> {
        match self {
            Network::Mainnet => Err(anyhow!("network is pre-mainnet")),
            Network::Testnet => Ok(reqwest::Url::from_str(TESTNET_PARENT_EVM_RPC_URL)?),
            Network::Devnet => Err(anyhow!("network has no parent")),
        }
    }

    pub fn parent_evm_gateway(&self) -> anyhow::Result<Address> {
        match self {
            Network::Mainnet => Err(anyhow!("network is pre-mainnet")),
            Network::Testnet => Ok(parse_address(TESTNET_PARENT_EVM_GATEWAY_ADDRESS)?),
            Network::Devnet => Err(anyhow!("network has no parent")),
        }
    }

    pub fn parent_evm_registry(&self) -> anyhow::Result<Address> {
        match self {
            Network::Mainnet => Err(anyhow!("network is pre-mainnet")),
            Network::Testnet => Ok(parse_address(TESTNET_PARENT_EVM_REGISTRY_ADDRESS)?),
            Network::Devnet => Err(anyhow!("network has no parent")),
        }
    }
}
