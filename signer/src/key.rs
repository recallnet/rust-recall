// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

use anyhow::Context;

pub use fendermint_crypto::SecretKey;

/// Parse [`SecretKey`] from a hex string.
pub fn parse_secret_key(hex_str: &str) -> anyhow::Result<SecretKey> {
    let mut hex_str = hex_str.trim();
    if hex_str.starts_with("0x") {
        hex_str = &hex_str[2..];
    }
    let raw_secret = hex::decode(hex_str).context("cannot decode hex private key")?;
    let sk = SecretKey::try_from(raw_secret).context("failed to parse secret key")?;
    Ok(sk)
}

/// Returns a new [`SecretKey`] from a thread-local random number generator, seeded by the system.
pub fn random_secretkey() -> SecretKey {
    let mut rng = rand::thread_rng();
    SecretKey::random(&mut rng)
}
