use std::error::Error;
use std::ops::Deref;

use ethers::prelude::TransactionReceipt;
use fendermint_crypto::SecretKey;
use fvm_shared::{address::Address, econ::TokenAmount};
use serde::Deserialize;
use serde_json::json;
use warp::{Filter, Rejection, Reply};

use adm_sdk::{account::Account, network::Network as SdkNetwork};

use crate::server::{
    shared::{get_faucet_wallet, with_private_key, BadRequest, BaseRequest},
    util::log_request_body,
};

/// Register request (essentially, equivalent to [`BaseRequest`]).
#[derive(Deserialize)]
pub struct RegisterRequest {
    #[serde(flatten)]
    pub base: BaseRequest,
}

impl std::fmt::Display for RegisterRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.base)
    }
}

impl Deref for RegisterRequest {
    type Target = BaseRequest;

    fn deref(&self) -> &Self::Target {
        &self.base
    }
}

/// Route filter for `/register` endpoint.
pub fn register_route(
    private_key: SecretKey,
) -> impl Filter<Extract = impl Reply, Error = Rejection> + Clone {
    warp::path("register")
        .and(warp::post())
        .and(warp::header::exact("content-type", "application/json"))
        .and(warp::body::json())
        .and(with_private_key(private_key.clone()))
        .and_then(handle_register)
}

/// Handles the `/register` request, first initializing the network.
pub async fn handle_register(
    req: RegisterRequest,
    private_key: SecretKey,
) -> anyhow::Result<impl Reply, Rejection> {
    req.network.init();
    log_request_body("register", &format!("{}", req));

    let res = register(req.network, req.address, private_key)
        .await
        .map_err(|e| {
            Rejection::from(BadRequest {
                message: format!("register error: {}", e),
            })
        })?;
    let json = json!(res);
    Ok(warp::reply::json(&json))
}

/// Registers an account on the subnet, creating the delegated EVM address (by
/// transferring 0 FIL).
pub async fn register(
    network: SdkNetwork,
    address: Address,
    private_key: SecretKey,
) -> anyhow::Result<TransactionReceipt, Box<dyn Error>> {
    let signer = get_faucet_wallet(private_key, network)?;
    let config = network.subnet_config(Default::default())?;
    let amount = TokenAmount::from_whole(0);
    let tx = Account::transfer(&signer, address, config, amount).await?;
    Ok(tx)
}
