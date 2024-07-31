use adm_sdk::network::Network as SdkNetwork;
use ethers::prelude::{
    abigen, Address, Http, LocalWallet, Middleware, Provider, Signer, SignerMiddleware, TxHash,
};
use fendermint_crypto::SecretKey;
use reqwest::Url;
use serde_json::json;
use std::convert::TryFrom;
use std::error::Error;
use std::sync::Arc;
use warp::{Filter, Rejection, Reply};

use crate::server::{
    shared::{with_private_key, with_token_address, BadRequest, BaseRequest},
    util::log_request_body,
};

abigen!(
    tHoku,
    r#"[{"inputs":[{"internalType":"address","name":"to","type":"address"},{"internalType":"uint256","name":"amount","type":"uint256"}],"name":"mint","outputs":[],"stateMutability":"nonpayable","type":"function"}]"#
);

/// Amount to send from the faucet to the user.
const FAUCET_AMOUNT: u64 = 5_000_000_000_000_000_000;

/// Route filter for `/send` endpoint.
pub fn send_route(
    private_key: SecretKey,
    token_address: Address,
) -> impl Filter<Extract = impl Reply, Error = Rejection> + Clone {
    warp::path("send")
        .and(warp::post())
        .and(warp::header::exact("content-type", "application/json"))
        .and(warp::body::json())
        .and(with_private_key(private_key.clone()))
        .and(with_token_address(token_address.clone()))
        .and_then(handle_send)
}

/// Handles the `/send` request.
pub async fn handle_send(
    req: BaseRequest,
    private_key: SecretKey,
    token_address: Address,
) -> anyhow::Result<impl Reply, Rejection> {
    log_request_body("send", &format!("{}", req));
    req.network.init();
    let address_bytes = req.address.into_payload().to_raw_bytes();
    let eth_address = Address::from_slice(&address_bytes[1..]);
    let res = send(req.network, eth_address, private_key, token_address)
        .await
        .map_err(|e| {
            Rejection::from(BadRequest {
                message: format!("send error: {}", e),
            })
        })?;
    let json = json!(res);
    Ok(warp::reply::json(&json))
}

/// Sends a transaction on the subnet.
pub async fn send(
    network: SdkNetwork,
    address: Address,
    private_key: SecretKey,
    token_address: Address,
) -> anyhow::Result<TxHash, Box<dyn Error>> {
    let node_url = network
        .parent_evm_rpc_url()
        .unwrap_or(Url::parse("http://127.0.0.1:8545")?);

    let provider = Provider::<Http>::try_from(node_url.to_string())?;
    let chain_id = provider.get_chainid().await?.as_u64();
    let private_key = private_key.serialize();
    let wallet = LocalWallet::from_bytes(private_key.as_slice())?.with_chain_id(chain_id);

    let client = SignerMiddleware::new(provider, wallet);
    let contract = tHoku::new(token_address, Arc::new(client));
    let receipt = contract
        .mint(address, FAUCET_AMOUNT.into())
        .send()
        .await?
        .clone();

    Ok(receipt)
}
