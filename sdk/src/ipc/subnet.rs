// Copyright 2024 ADM Contributors
// SPDX-License-Identifier: Apache-2.0, MIT

use std::time::Duration;

use fvm_shared::address::Address;
use reqwest::Url;

use adm_signer::SubnetID;

/// The EVM subnet config parameters.
#[derive(Debug, Clone)]
pub struct EVMSubnet {
    /// The target subnet ID.
    pub id: SubnetID,
    /// The EVM RPC provider endpoint.
    pub provider_http: Url,
    /// The EVM RPC provider request timeout.
    pub provider_timeout: Option<Duration>,
    /// The EVM RPC provider authorization token.
    pub auth_token: Option<String>,
    /// The EVM registry contract address.
    pub registry_addr: Address,
    /// The EVM gateway contract address.
    pub gateway_addr: Address,
    /// The EVM supply source contract address.
    pub supply_source: Option<Address>,
}
