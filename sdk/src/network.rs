// Copyright 2024 Hoku Contributors
// SPDX-License-Identifier: Apache-2.0, MIT

use std::env;
use std::str::FromStr;
use std::time::Duration;
use std::{collections::HashMap, fmt::Display};

use anyhow::{anyhow, Context};
use fvm_shared::address::{self, Address, Error, Network as FvmNetwork};
use lazy_static::lazy_static;
use serde::{Deserialize, Deserializer};
use tendermint_rpc::Url;

use hoku_provider::util::parse_address;
use hoku_signer::SubnetID;
use tokio::sync::OnceCell;

use crate::ipc::subnet::EVMSubnet;

const TESTNET_SUBNET_ID: &str = "/r314159/t410fvamrbjioufgzoyojg2x3nwdo26t6xucxoxl47yq"; // chain ID: 2938118273996536
const LOCALNET_SUBNET_ID: &str = "/r31337/t410fkzrz3mlkyufisiuae3scumllgalzuu3wxlxa2ly"; // chain ID: 4362550583360910
const DEVNET_SUBNET_ID: &str = "test";
const IGNITION_SUBNET_ID: &str = "/r314159/t410f3oewkcvacaaydfn4v6diulzise26cpfolfj7heq"; // chain ID: 3258443211374980

const TESTNET_RPC_URL: &str = "https://api.n1.hoku.sh";
const LOCALNET_RPC_URL: &str = "http://127.0.0.1:26657";
const IGNITION_RPC_URL: &str = "https://api-ignition-0.hoku.sh";

const RPC_TIMEOUT: Duration = Duration::from_secs(60);

const TESTNET_EVM_RPC_URL: &str = "https://evm-api.n1.hoku.sh";
const LOCALNET_EVM_RPC_URL: &str = "http://127.0.0.1:8645";
const IGNITION_EVM_RPC_URL: &str = "https://evm-ignition-0.hoku.sh";
const DEVNET_EVM_RPC_URL: &str = "http://127.0.0.1:8545";

const TESTNET_EVM_GATEWAY_ADDRESS: &str = "0x77aa40b105843728088c0132e43fc44348881da8";
const TESTNET_EVM_REGISTRY_ADDRESS: &str = "0x74539671a1d2f1c8f200826baba665179f53a1b7";
const TESTNET_EVM_SUPPLY_SOURCE_ADDRESS: &str = "0x8e3Fd2b47e564E7D636Fa80082f286eD038BE54b";
const LOCALNET_EVM_GATEWAY_ADDRESS: &str = "0x77aa40b105843728088c0132e43fc44348881da8";
const LOCALNET_EVM_REGISTRY_ADDRESS: &str = "0x74539671a1d2f1c8f200826baba665179f53a1b7";
const LOCALNET_EVM_SUPPLY_SOURCE_ADDRESS: &str = "0xE6E340D132b5f46d1e472DebcD681B2aBc16e57E";
const IGNITION_EVM_GATEWAY_ADDRESS: &str = "0x77aa40b105843728088c0132e43fc44348881da8";
const IGNITION_EVM_REGISTRY_ADDRESS: &str = "0x74539671a1d2f1c8f200826baba665179f53a1b7";
const IGNITION_EVM_SUPPLY_SOURCE_ADDRESS: &str = "0x20d8a696091153c4d4816ba1fdefe113f71e0905";
const DEVNET_EVM_GATEWAY_ADDRESS: &str = "0x77aa40b105843728088c0132e43fc44348881da8";
const DEVNET_EVM_REGISTRY_ADDRESS: &str = "0x74539671a1d2f1c8f200826baba665179f53a1b7";

const TESTNET_PARENT_EVM_RPC_URL: &str = "https://api.calibration.node.glif.io/rpc/v1";
const TESTNET_PARENT_EVM_GATEWAY_ADDRESS: &str = "0xe17B86E7BEFC691DAEfe2086e56B86D4253f3294";
const TESTNET_PARENT_EVM_REGISTRY_ADDRESS: &str = "0xe87AFBEC26f0fdAC69e4256dC1935bEab1e0855E";
const LOCALNET_PARENT_EVM_RPC_URL: &str = "http://127.0.0.1:8545";
const LOCALNET_PARENT_EVM_GATEWAY_ADDRESS: &str = "0x9A676e781A523b5d0C0e43731313A708CB607508";
const LOCALNET_PARENT_EVM_REGISTRY_ADDRESS: &str = "0x4ed7c70F96B99c776995fB64377f0d4aB3B0e1C1";
const IGNITION_PARENT_EVM_RPC_URL: &str = "https://api.calibration.node.glif.io/rpc/v1";
const IGNITION_PARENT_EVM_GATEWAY_ADDRESS: &str = "0xF8Abf46A1114d3B44d18F2A96D850e36FC6Ee94E";
const IGNITION_PARENT_EVM_REGISTRY_ADDRESS: &str = "0x0bb143a180b61ae6b1872bbf99dBe261A2aDde40";

const TESTNET_OBJECT_API_URL: &str = "https://object-api.n1.hoku.sh";
const LOCALNET_OBJECT_API_URL: &str = "http://127.0.0.1:8001";
const IGNITION_OBJECT_API_URL: &str = "https://object-api-ignition-0.hoku.sh";

const HOKU_NETWORK_CONFIGS_URL: &str = "http://127.0.0.1:3000/network-definitions.json";
const HOKU_NETWORK_CONFIGS_URL_ENVVAR: &str = "HOKU_NETWORK_CONFIGS_URL";

/// Options for [`EVMSubnet`] configurations.
#[derive(Debug, Clone)]
pub struct SubnetOptions {
    /// The EVM RPC provider request timeout.
    pub evm_rpc_timeout: Duration,
    /// The EVM RPC provider authorization token.
    pub evm_rpc_auth_token: Option<String>,
}

impl Default for SubnetOptions {
    fn default() -> Self {
        Self {
            evm_rpc_timeout: RPC_TIMEOUT,
            evm_rpc_auth_token: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct NetworkConfig {
    #[serde(deserialize_with = "deserialize_subnet_id")]
    pub subnet_id: SubnetID,
    pub rpc_url: Url,
    pub object_api_url: Url,
    pub evm_rpc_url: reqwest::Url,
    pub evm_gateway_address: Address,
    pub evm_registry_address: Address,
    pub parent_network_config: Option<ParentNetworkConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ParentNetworkConfig {
    pub evm_rpc_url: reqwest::Url,
    pub evm_gateway_address: Address,
    pub evm_registry_address: Address,
    pub evm_supply_source_address: Address,
}

fn deserialize_subnet_id<'de, D>(deserializer: D) -> Result<SubnetID, D::Error>
where
    D: Deserializer<'de>,
{
    let x = String::deserialize(deserializer)?;
    SubnetID::from_str(&x).map_err(serde::de::Error::custom)
}

impl NetworkConfig {
    async fn get_remote(network_name: &str) -> anyhow::Result<Self> {
        let url = NetworkConfig::network_definitions_url();

        let resp = reqwest::get(&url).await.context(format!(
            "failed to download network definitions from {}",
            &url
        ))?;
        let mut network_configs: HashMap<String, NetworkConfig> = resp
            .json()
            .await
            .context(format!("invalid JSON content downloaded from {}", &url))?;
        network_configs.remove(network_name).ok_or(anyhow!(
            "no such network '{}' at {}",
            network_name,
            &url
        ))
    }

    /// Returns the URL read from HOKU_NETWORK_CONFIGS_URL envvar or the default URL.
    /// This feature is intended for development use, and the environment variable is intentionally undocumented for end users.
    fn network_definitions_url() -> String {
        env::var(HOKU_NETWORK_CONFIGS_URL_ENVVAR).unwrap_or(HOKU_NETWORK_CONFIGS_URL.to_owned())
    }

    pub fn subnet_config(&self, options: SubnetOptions) -> EVMSubnet {
        EVMSubnet {
            id: self.subnet_id.clone(),
            provider_http: self.evm_rpc_url.clone(),
            provider_timeout: Some(options.evm_rpc_timeout),
            auth_token: options.evm_rpc_auth_token,
            registry_addr: self.evm_registry_address,
            gateway_addr: self.evm_gateway_address,
            supply_source: None,
        }
    }

    pub fn parent_subnet_config(&self, options: SubnetOptions) -> Option<EVMSubnet> {
        self.parent_network_config.as_ref().map(|parent| EVMSubnet {
            id: self
                .subnet_id
                .parent()
                .expect("subnet does not have parent"),
            provider_http: parent.evm_rpc_url.clone(),
            provider_timeout: Some(options.evm_rpc_timeout),
            auth_token: options.evm_rpc_auth_token,
            registry_addr: parent.evm_registry_address,
            gateway_addr: parent.evm_gateway_address,
            supply_source: Some(parent.evm_supply_source_address),
        })
    }
}

lazy_static! {
    // This is a temporary workaround to be able to use network configurations downloaded from a URL and not breaking current code.
    // Remove it after Network::static_config has been removed.
    static ref CURRENT_NETWORK_CONFIG: OnceCell<NetworkConfig> = OnceCell::new();
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
    /// Network presets for Ignition testnet.
    Ignition,
    // /// Network presets will be downloaded from HOKU_NETWORK_CONFIGS_URL
    Remote(String),
}

impl Network {
    /// Sets the current [`FvmNetwork`].
    /// Note: This _must_ be called before using the SDK.
    pub async fn init(&self) -> anyhow::Result<&Self> {
        match self {
            Network::Mainnet => address::set_current_network(FvmNetwork::Mainnet),
            _ => address::set_current_network(FvmNetwork::Testnet),
        }
        CURRENT_NETWORK_CONFIG.set(self.get_config().await?)?;
        Ok(self)
    }

    pub async fn get_config(&self) -> anyhow::Result<NetworkConfig> {
        Ok(match self {
            Network::Mainnet => todo!(),
            Network::Testnet => NetworkConfig {
                subnet_id: SubnetID::from_str(TESTNET_SUBNET_ID).unwrap(),
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
                subnet_id: SubnetID::from_str(LOCALNET_SUBNET_ID).unwrap(),
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
            Network::Ignition => NetworkConfig {
                subnet_id: SubnetID::from_str(IGNITION_SUBNET_ID).unwrap(),
                rpc_url: Url::from_str(IGNITION_RPC_URL).unwrap(),
                object_api_url: Url::from_str(IGNITION_OBJECT_API_URL).unwrap(),
                evm_rpc_url: reqwest::Url::from_str(IGNITION_EVM_RPC_URL).unwrap(),
                evm_gateway_address: parse_address(IGNITION_EVM_GATEWAY_ADDRESS).unwrap(),
                evm_registry_address: parse_address(IGNITION_EVM_REGISTRY_ADDRESS).unwrap(),
                parent_network_config: Some(ParentNetworkConfig {
                    evm_rpc_url: reqwest::Url::from_str(IGNITION_PARENT_EVM_RPC_URL).unwrap(),
                    evm_gateway_address: parse_address(IGNITION_PARENT_EVM_GATEWAY_ADDRESS)
                        .unwrap(),
                    evm_registry_address: parse_address(IGNITION_PARENT_EVM_REGISTRY_ADDRESS)
                        .unwrap(),
                    evm_supply_source_address: parse_address(IGNITION_EVM_SUPPLY_SOURCE_ADDRESS)
                        .unwrap(),
                }),
            },
            Network::Remote(name) => NetworkConfig::get_remote(name).await?,
        })
    }

    #[deprecated(note = "Use get_config")]
    fn static_config(&self) -> &'static NetworkConfig {
        CURRENT_NETWORK_CONFIG.get().unwrap()
    }

    /// Returns the network [`SubnetID`].
    pub fn subnet_id(&self) -> anyhow::Result<SubnetID> {
        Ok(self.static_config().subnet_id.clone())
    }

    /// Returns the network [`EVMSubnet`] parent configuration.
    pub fn parent_subnet_config(&self, options: SubnetOptions) -> anyhow::Result<EVMSubnet> {
        self.static_config()
            .parent_subnet_config(options)
            .ok_or(anyhow!("network is pre-mainnet"))
    }
}

#[tokio::test]
async fn correct_network_definitions() -> anyhow::Result<()> {
    let _ = Network::Devnet.get_config().await?;
    let _ = Network::Localnet.get_config().await?;
    let _ = Network::Testnet.get_config().await?;
    let _ = Network::Ignition.get_config().await?;
    Ok(())
}

impl FromStr for Network {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "mainnet" => Ok(Network::Mainnet),
            "testnet" => Ok(Network::Testnet),
            "localnet" => Ok(Network::Localnet),
            "devnet" => Ok(Network::Devnet),
            "ignition" => Ok(Network::Ignition),
            s => s
                .strip_prefix("remote:")
                .map(|network_name| Network::Remote(network_name.to_owned()))
                .ok_or(Error::UnknownNetwork.to_string()),
        }
    }
}

#[test]
fn parse_network_name() {
    assert_eq!(Network::from_str("mainnet").unwrap(), Network::Mainnet);
    assert_eq!(
        Network::from_str("remote:abc").unwrap(),
        Network::Remote("abc".to_owned())
    );
}

impl Display for Network {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Network::Mainnet => write!(f, "mainnet"),
            Network::Testnet => write!(f, "testnet"),
            Network::Localnet => write!(f, "localnet"),
            Network::Devnet => write!(f, "devnet"),
            Network::Ignition => write!(f, "ignition"),
            Network::Remote(name) => write!(f, "{}", name),
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
