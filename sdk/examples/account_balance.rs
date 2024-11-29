// Copyright 2024 Hoku Contributors
// SPDX-License-Identifier: Apache-2.0, MIT

use std::env;

use anyhow::anyhow;
use ethers::utils::hex::ToHexExt;

use hoku_sdk::{account::Account, network::Network};
use hoku_signer::{key::parse_secret_key, AccountKind, Signer, Wallet};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        return Err(anyhow!("missing hex-encoded private key"));
    }
    let pk_kex = &args[1];
    let pk = parse_secret_key(pk_kex)?;

    // Use testnet network defaults
    let cfg = Network::Testnet.get_config();

    // Setup local wallet using private key from arg
    let signer = Wallet::new_secp256k1(pk, AccountKind::Ethereum, cfg.subnet_id.parent()?)?;

    // Deposit some calibration funds into the subnet
    // Note: The debit account _must_ have Calibration
    let balance = Account::balance(&signer, cfg.subnet_config()).await?;

    println!(
        "Balance of {}: {}",
        signer.evm_address()?.encode_hex_with_prefix(),
        balance,
    );

    Ok(())
}
