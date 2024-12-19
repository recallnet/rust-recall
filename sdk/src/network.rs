// Copyright 2024 Hoku Contributors
// SPDX-License-Identifier: Apache-2.0, MIT

use std::fmt::Display;
use std::str::FromStr;
use std::time::Duration;

use fvm_shared::{
    address::{self, Address, Error, Network as FvmNetwork},
    chainid::ChainID,
};
use serde::{Deserialize, Deserializer};
use tendermint_rpc::Url;

use hoku_provider::util::parse_address;
use hoku_signer::SubnetID;

use crate::ipc::subnet::EVMSubnet;

const DEFAULT_RPC_TIMEOUT: Duration = Duration::from_secs(60);

const DEVNET_SUBNET_ID: &str = "test";
const DEVNET_EVM_RPC_URL: &str = "http://127.0.0.1:8545";
const DEVNET_EVM_GATEWAY_ADDRESS: &str = "0x77aa40b105843728088c0132e43fc44348881da8";
const DEVNET_EVM_REGISTRY_ADDRESS: &str = "0x74539671a1d2f1c8f200826baba665179f53a1b7";

const LOCALNET_RPC_URL: &str = "http://127.0.0.1:26657";
const LOCALNET_SUBNET_ID: &str = "/r31337/t410f6gbdxrbehnaeeo4mrq7wc5hgq6smnefys4qanwi"; // chain ID: 534485604105794
const LOCALNET_CHAIN_ID: u64 = 248163216;
const LOCALNET_EVM_RPC_URL: &str = "http://127.0.0.1:8645";
const LOCALNET_OBJECT_API_URL: &str = "http://127.0.0.1:8001";
const LOCALNET_EVM_GATEWAY_ADDRESS: &str = "0x77aa40b105843728088c0132e43fc44348881da8";
const LOCALNET_EVM_REGISTRY_ADDRESS: &str = "0x74539671a1d2f1c8f200826baba665179f53a1b7";
const LOCALNET_EVM_SUPPLY_SOURCE_ADDRESS: &str = "0x4A679253410272dd5232B3Ff7cF5dbB88f295319";
const LOCALNET_PARENT_EVM_RPC_URL: &str = "http://127.0.0.1:8545";
const LOCALNET_PARENT_EVM_GATEWAY_ADDRESS: &str = "0x9A676e781A523b5d0C0e43731313A708CB607508";
const LOCALNET_PARENT_EVM_REGISTRY_ADDRESS: &str = "0x322813Fd9A801c5507c9de605d63CEA4f2CE6c44";

// Ignition
const TESTNET_RPC_URL: &str = "https://api-ignition-0.hoku.sh";
const TESTNET_SUBNET_ID: &str = "/r314159/t410f2mjuvgks4vlwbmtfmwohbaqo5v2w5fbcr5zf7li";
const TESTNET_CHAIN_ID: u64 = 2481632;
const TESTNET_OBJECT_API_URL: &str = "https://object-api-ignition-0.hoku.sh";
const TESTNET_EVM_RPC_URL: &str = "https://evm-ignition-0.hoku.sh";
const TESTNET_PARENT_EVM_RPC_URL: &str = "https://api.calibration.node.glif.io/rpc/v1";
const TESTNET_PARENT_EVM_GATEWAY_ADDRESS: &str = "0xF8Abf46A1114d3B44d18F2A96D850e36FC6Ee94E";
const TESTNET_PARENT_EVM_REGISTRY_ADDRESS: &str = "0x0bb143a180b61ae6b1872bbf99dBe261A2aDde40";
const TESTNET_EVM_GATEWAY_ADDRESS: &str = "0x77aa40b105843728088c0132e43fc44348881da8";
const TESTNET_EVM_REGISTRY_ADDRESS: &str = "0x74539671a1d2f1c8f200826baba665179f53a1b7";
const TESTNET_EVM_SUPPLY_SOURCE_ADDRESS: &str = "0x20d8a696091153c4d4816ba1fdefe113f71e0905";

#[derive(Debug, Clone)]
pub struct NetworkConfig {
    pub subnet_id: SubnetID,
    pub rpc_url: Url,
    pub object_api_url: Url,
    pub evm_rpc_url: reqwest::Url,
    pub evm_gateway_address: Address,
    pub evm_registry_address: Address,
    pub parent_network_config: Option<ParentNetworkConfig>,
}

#[derive(Debug, Clone)]
pub struct ParentNetworkConfig {
    pub evm_rpc_url: reqwest::Url,
    pub evm_gateway_address: Address,
    pub evm_registry_address: Address,
    pub evm_supply_source_address: Address,
}

impl NetworkConfig {
    pub fn subnet_config(&self) -> EVMSubnet {
        EVMSubnet {
            id: self.subnet_id.clone(),
            provider_http: self.evm_rpc_url.clone(),
            provider_timeout: Some(DEFAULT_RPC_TIMEOUT),
            auth_token: None,
            registry_addr: self.evm_registry_address,
            gateway_addr: self.evm_gateway_address,
            supply_source: None,
        }
    }

    pub fn parent_subnet_config(&self) -> Option<EVMSubnet> {
        self.parent_network_config.as_ref().map(|parent| EVMSubnet {
            id: self
                .subnet_id
                .parent()
                .expect("subnet does not have parent"),
            provider_http: parent.evm_rpc_url.clone(),
            provider_timeout: Some(DEFAULT_RPC_TIMEOUT),
            auth_token: None,
            registry_addr: parent.evm_registry_address,
            gateway_addr: parent.evm_gateway_address,
            supply_source: Some(parent.evm_supply_source_address),
        })
    }
}

/// Network presets for a subnet configuration and RPC URLs.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Network {
    /// Network presets for mainnet.
    Mainnet,
    /// Network presets for Calibration (default pre-mainnet).
    Testnet,
    /// Network presets for a local three-node network.
    Localnet,
    /// Network presets for local development.
    Devnet,
}

impl Network {
    /// Sets the current [`FvmNetwork`].
    /// Note: This _must_ be called before using the SDK.
    pub fn init(&self) -> &Self {
        match self {
            Network::Mainnet => address::set_current_network(FvmNetwork::Mainnet),
            _ => address::set_current_network(FvmNetwork::Testnet),
        }
        self
    }

    pub fn get_config(&self) -> NetworkConfig {
        self.init();
        match self {
            Network::Mainnet => todo!(),
            Network::Testnet => NetworkConfig {
                subnet_id: SubnetID::from_str(TESTNET_SUBNET_ID)
                    .unwrap()
                    .with_chain_id(ChainID::from(TESTNET_CHAIN_ID)),
                rpc_url: Url::from_str(TESTNET_RPC_URL).unwrap(),
                object_api_url: Url::from_str(TESTNET_OBJECT_API_URL).unwrap(),
                evm_rpc_url: reqwest::Url::from_str(TESTNET_EVM_RPC_URL).unwrap(),
                evm_gateway_address: parse_address(TESTNET_EVM_GATEWAY_ADDRESS).unwrap(),
                evm_registry_address: parse_address(TESTNET_EVM_REGISTRY_ADDRESS).unwrap(),
                parent_network_config: Some(ParentNetworkConfig {
                    evm_rpc_url: reqwest::Url::from_str(TESTNET_PARENT_EVM_RPC_URL).unwrap(),
                    evm_gateway_address: parse_address(TESTNET_PARENT_EVM_GATEWAY_ADDRESS).unwrap(),
                    evm_registry_address: parse_address(TESTNET_PARENT_EVM_REGISTRY_ADDRESS)
                        .unwrap(),
                    evm_supply_source_address: parse_address(TESTNET_EVM_SUPPLY_SOURCE_ADDRESS)
                        .unwrap(),
                }),
            },
            Network::Localnet => NetworkConfig {
                subnet_id: SubnetID::from_str(LOCALNET_SUBNET_ID)
                    .unwrap()
                    .with_chain_id(ChainID::from(LOCALNET_CHAIN_ID)),
                rpc_url: Url::from_str(LOCALNET_RPC_URL).unwrap(),
                object_api_url: Url::from_str(LOCALNET_OBJECT_API_URL).unwrap(),
                evm_rpc_url: reqwest::Url::from_str(LOCALNET_EVM_RPC_URL).unwrap(),
                evm_gateway_address: parse_address(LOCALNET_EVM_GATEWAY_ADDRESS).unwrap(),
                evm_registry_address: parse_address(LOCALNET_EVM_REGISTRY_ADDRESS).unwrap(),
                parent_network_config: Some(ParentNetworkConfig {
                    evm_rpc_url: reqwest::Url::from_str(LOCALNET_PARENT_EVM_RPC_URL).unwrap(),
                    evm_gateway_address: parse_address(LOCALNET_PARENT_EVM_GATEWAY_ADDRESS)
                        .unwrap(),
                    evm_registry_address: parse_address(LOCALNET_PARENT_EVM_REGISTRY_ADDRESS)
                        .unwrap(),
                    evm_supply_source_address: parse_address(LOCALNET_EVM_SUPPLY_SOURCE_ADDRESS)
                        .unwrap(),
                }),
            },
            Network::Devnet => NetworkConfig {
                subnet_id: SubnetID::from_str(DEVNET_SUBNET_ID).unwrap(),
                rpc_url: Url::from_str(LOCALNET_RPC_URL).unwrap(),
                object_api_url: Url::from_str(LOCALNET_OBJECT_API_URL).unwrap(),
                evm_rpc_url: reqwest::Url::from_str(DEVNET_EVM_RPC_URL).unwrap(),
                evm_gateway_address: parse_address(DEVNET_EVM_GATEWAY_ADDRESS).unwrap(),
                evm_registry_address: parse_address(DEVNET_EVM_REGISTRY_ADDRESS).unwrap(),
                parent_network_config: None,
            },
        }
    }
}

#[test]
fn correct_network_definitions() {
    let _ = Network::Devnet.get_config();
    let _ = Network::Localnet.get_config();
    let _ = Network::Testnet.get_config();
}

impl FromStr for Network {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "mainnet" => Ok(Network::Mainnet),
            "testnet" => Ok(Network::Testnet),
            "localnet" => Ok(Network::Localnet),
            "devnet" => Ok(Network::Devnet),
            _ => Err(Error::UnknownNetwork.to_string()),
        }
    }
}

impl Display for Network {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Network::Mainnet => write!(f, "mainnet"),
            Network::Testnet => write!(f, "testnet"),
            Network::Localnet => write!(f, "localnet"),
            Network::Devnet => write!(f, "devnet"),
        }
    }
}

impl<'de> Deserialize<'de> for Network {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s: String = String::deserialize(deserializer)?;
        Network::from_str(&s).map_err(serde::de::Error::custom)
    }
}
