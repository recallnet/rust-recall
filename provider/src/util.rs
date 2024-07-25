// Copyright 2024 ADM Contributors
// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

use fendermint_vm_message::query::FvmQueryHeight;
use std::str::FromStr;

use fvm_shared::{
    address::{Address, Error, Network},
    bigint::BigInt,
    econ::TokenAmount,
};
use ipc_api::{ethers_address_to_fil_address, evm::payload_to_evm_address};

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

/// Converts f-address to eth-address. Only delegated address is supported.
pub fn get_delegated_address(a: Address) -> anyhow::Result<ethers::types::Address> {
    payload_to_evm_address(a.payload())
}

/// We only support up to 9 decimal digits for transaction.
const FIL_AMOUNT_NANO_DIGITS: u32 = 9;

/// Parse token amount from string.
pub fn parse_token_amount(s: &str) -> anyhow::Result<TokenAmount> {
    let f: f64 = s.parse()?;
    // no rounding, just the integer part
    let nano = f64::trunc(f * (10u64.pow(FIL_AMOUNT_NANO_DIGITS) as f64));
    Ok(TokenAmount::from_nano(nano as u128))
}

/// Parse token amount in attoFIL (10**18) from string.
pub fn parse_token_amount_from_atto(s: &str) -> anyhow::Result<TokenAmount> {
    Ok(TokenAmount::from_atto(BigInt::from_str(s)?))
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
