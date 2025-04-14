// Copyright 2025 Recall Contributors
// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

use std::str::FromStr;

use anyhow::anyhow;
use fvm_shared::{
    address::{Address, Error, Network, Payload},
    bigint::{BigInt, BigUint},
    econ::TokenAmount,
};
use recall_fendermint_actor_blobs_shared::state::{Credit, TokenCreditRate};
use recall_fendermint_vm_actor_interface::eam::EthAddress;
use recall_fendermint_vm_message::query::FvmQueryHeight;
use rust_decimal::Decimal;

pub use recall_ipc_api::{ethers_address_to_fil_address, evm::payload_to_evm_address};

/// Parse an f/eth-address from string.
pub fn parse_address(s: &str) -> anyhow::Result<Address> {
    let addr = Network::Mainnet
        .parse_address(s)
        .or_else(|e| match e {
            Error::UnknownNetwork => Network::Testnet.parse_address(s),
            _ => Err(e),
        })
        .or_else(|_| {
            let addr = ethers::types::Address::from_str(s)?;
            ethers_address_to_fil_address(&addr)
        })?;
    Ok(addr)
}

/// Converts f-address to eth-address. Only masked ID and delegated addresses are supported.
pub fn get_eth_address(a: Address) -> anyhow::Result<ethers::types::Address> {
    match a.payload() {
        Payload::Delegated(delegated) => {
            let slice = delegated.subaddress();
            Ok(ethers::types::Address::from_slice(&slice[0..20]))
        }
        Payload::ID(id) => Ok(EthAddress::from_id(*id).0.into()),
        _ => Err(anyhow!("address provided is not masked ID or delegated")),
    }
}

/// Parse the token amount from string.
pub fn parse_token_amount(s: &str) -> anyhow::Result<TokenAmount> {
    let decimal = Decimal::from_str(s)?;

    // Scale the decimal to atto (10^18)
    let decimal_in_attos = decimal
        .checked_mul(Decimal::new(1_000_000_000_000_000_000, 0))
        .ok_or(anyhow!("overflow occurred when scaling '{}'", s))?;

    Ok(TokenAmount::from_atto(BigInt::from_str(
        &decimal_in_attos.trunc().to_string(),
    )?))
}

/// Parse the token amount in attoRECALL (10**18) from string.
pub fn parse_token_amount_from_atto(s: &str) -> anyhow::Result<TokenAmount> {
    Ok(TokenAmount::from_atto(BigInt::from_str(s)?))
}

/// Parse the credit amount from string.
pub fn parse_credit_amount(s: &str) -> anyhow::Result<Credit> {
    Ok(Credit::from_whole(BigInt::from_str(s)?))
}

/// Parse the token to credit rate.
pub fn parse_token_credit_rate(s: &str) -> anyhow::Result<TokenCreditRate> {
    Ok(TokenCreditRate::from(BigUint::from_str(s)?))
}

/// Parse query height from string.
pub fn parse_query_height(s: &str) -> anyhow::Result<FvmQueryHeight> {
    let height = match s.to_lowercase().as_str() {
        "committed" => FvmQueryHeight::Committed,
        "pending" => FvmQueryHeight::Pending,
        _ => FvmQueryHeight::Height(s.parse::<u64>()?),
    };
    Ok(height)
}

/// Parse metadata from string.
pub fn parse_metadata(s: &str) -> anyhow::Result<(String, String)> {
    let pos = s
        .find('=')
        .ok_or_else(|| anyhow::anyhow!("Expected KEY=VALUE format but `=` not found in `{}`", s))?;
    let key = s[..pos].to_string();
    let val = s[pos + 1..].to_string();
    Ok((key, val))
}

/// Parse metadata from string accepting empty values.
pub fn parse_metadata_optional(s: &str) -> anyhow::Result<(String, Option<String>)> {
    match s.find('=') {
        Some(pos) => {
            let key = s[..pos].to_string();
            let val = s[pos + 1..].to_string();
            if val.is_empty() {
                return Err(anyhow::anyhow!("empty VALUE provided"));
            }

            Ok((key, Some(val)))
        }
        None => Ok((s.to_string(), None)),
    }
}
