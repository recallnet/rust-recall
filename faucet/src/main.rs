use std::net::{SocketAddr, ToSocketAddrs};

use anyhow::anyhow;
use clap::Parser;
use fendermint_crypto::SecretKey;
use stderrlog::Timestamp;

use adm_signer::key::parse_secret_key;

use crate::server::run;

mod server;

#[derive(Clone, Debug, Parser)]
#[command(name = "adm_faucet", author, version, about, long_about = None)]
struct Cli {
    /// Wallet private key (ECDSA, secp256k1) for sending faucet funds.
    #[arg(short, long, env, value_parser = parse_secret_key)]
    private_key: SecretKey,
    /// Faucet `host:port` string for running the HTTP server.
    #[arg(long, env, value_parser = parse_faucet_url)]
    listen: SocketAddr,
    /// Logging verbosity (repeat for more verbose logging).
    #[arg(short, long, env, action = clap::ArgAction::Count)]
    verbosity: u8,
    /// Silence logging.
    #[arg(short, long, env, default_value_t = false)]
    quiet: bool,
}

/// Parse the [`SocketAddr`] from a faucet URL string.
fn parse_faucet_url(listen: &str) -> anyhow::Result<SocketAddr> {
    match listen.to_socket_addrs()?.next() {
        Some(addr) => Ok(addr),
        None => Err(anyhow!(
            "failed to convert to any socket address: {}",
            listen
        )),
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    stderrlog::new()
        .module(module_path!())
        .quiet(cli.quiet)
        .verbosity(cli.verbosity as usize)
        .timestamp(Timestamp::Millisecond)
        .init()
        .unwrap();

    run(cli).await
}
