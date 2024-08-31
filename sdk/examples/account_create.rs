// Copyright 2024 Hoku Contributors
// SPDX-License-Identifier: Apache-2.0, MIT

use ethers::utils::hex::ToHexExt;
use fendermint_vm_actor_interface::eam::EthAddress;
use fvm_shared::address::Address;

use hoku_sdk::network::Network;
use hoku_signer::key::random_secretkey;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Use testnet network defaults
    Network::Testnet.init();

    let sk = random_secretkey();
    let pk = sk.public_key().serialize();
    let eth_address = EthAddress::new_secp256k1(&pk)?;
    let address = Address::from(eth_address);
    let sk_hex = hex::encode(sk.serialize());

    println!("Private key: {}", sk_hex);
    println!("Address: {}", eth_address.encode_hex_with_prefix());
    println!("FVM address: {}", address);

    Ok(())
}
