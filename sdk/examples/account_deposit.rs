// Copyright 2024 Hoku Contributors
// SPDX-License-Identifier: Apache-2.0, MIT

use std::env;

use anyhow::anyhow;
use ethers::utils::hex::ToHexExt;

use hoku_provider::fvm_shared::econ::TokenAmount;
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
    // Note: The debit account _must_ hold at least 1 Calibration HOKU for the deposit
    // plus enough to cover the transaction fee.
    // Go to the faucet at https://faucet.calibnet.chainsafe-fil.io/ to get yourself some HOKU.
    let cfg = Network::Testnet.get_config();

    // Setup local wallet using private key from arg
    let signer = Wallet::new_secp256k1(pk, AccountKind::Ethereum, cfg.subnet_id.parent()?)?;

    // Deposit some calibration funds into the subnet
    // Note: The debit account _must_ have Calibration
    let tx = Account::deposit(
        &signer,
        signer.address(),
        cfg.parent_subnet_config()
            .ok_or(anyhow!("network does not have parent"))?,
        cfg.subnet_id,
        TokenAmount::from_whole(1),
    )
    .await?;

    println!(
        "Deposited 1 HOKU to {}",
        signer.eth_address()?.encode_hex_with_prefix()
    );
    println!(
        "Transaction hash: 0x{}",
        hex::encode(tx.transaction_hash.to_fixed_bytes())
    );

    Ok(())
}
