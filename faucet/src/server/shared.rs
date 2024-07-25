use std::convert::Infallible;

use fendermint_crypto::SecretKey;
use fvm_shared::address::Address;
use serde::{Deserialize, Deserializer, Serialize};
use warp::{http::StatusCode, Filter, Rejection, Reply};

use adm_provider::util::parse_address;
use adm_sdk::network::Network as SdkNetwork;
use adm_signer::{AccountKind, Wallet};

/// Generic base request for all routes.
#[derive(Deserialize)]
pub struct BaseRequest {
    pub network: SdkNetwork,
    #[serde(deserialize_with = "deserialize_address")]
    pub address: Address,
}

impl std::fmt::Display for BaseRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "network: {:?}, address: {}", self.network, self.address)
    }
}

/// Custom deserializer to allow for FVM or EVM addresses to be used as input.
fn deserialize_address<'de, D>(deserializer: D) -> Result<Address, D::Error>
where
    D: Deserializer<'de>,
{
    let s: String = String::deserialize(deserializer)?;
    parse_address(s.as_str()).map_err(serde::de::Error::custom)
}

/// Generic request error.
#[derive(Clone, Debug)]
pub struct BadRequest {
    pub message: String,
}

impl warp::reject::Reject for BadRequest {}

/// Custom error message with status code.
#[derive(Clone, Debug, Serialize)]
struct ErrorMessage {
    code: u16,
    message: String,
}

/// Rejection handler.
pub async fn handle_rejection(err: Rejection) -> Result<impl Reply, Infallible> {
    let (code, message) = if err.is_not_found() {
        (StatusCode::NOT_FOUND, "Not Found".to_string())
    } else if let Some(e) = err.find::<BadRequest>() {
        (StatusCode::BAD_REQUEST, e.message.clone())
    } else if let Some(e) = err.find::<warp::filters::body::BodyDeserializeError>() {
        (
            StatusCode::BAD_REQUEST,
            format!("Invalid Request Body: {}", e),
        )
    } else if err.find::<warp::reject::InvalidHeader>().is_some() {
        (StatusCode::BAD_REQUEST, "Invalid Header Value".to_string())
    } else if err.find::<warp::reject::MethodNotAllowed>().is_some() {
        (
            StatusCode::METHOD_NOT_ALLOWED,
            "Method Not Allowed".to_string(),
        )
    } else {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Internal Server Error".to_string(),
        )
    };

    let reply = warp::reply::json(&ErrorMessage {
        code: code.as_u16(),
        message,
    });
    Ok(warp::reply::with_status(reply, code))
}

/// Filter to pass the private key to the request handler.
pub fn with_private_key(
    private_key: SecretKey,
) -> impl Filter<Extract = (SecretKey,), Error = Infallible> + Clone {
    warp::any().map(move || private_key.clone())
}

/// Get the faucet wallet that will send transactions on the subnet.
pub fn get_faucet_wallet(private_key: SecretKey, network: SdkNetwork) -> anyhow::Result<Wallet> {
    let signer = Wallet::new_secp256k1(private_key, AccountKind::Ethereum, network.subnet_id()?)?;
    Ok(signer)
}
