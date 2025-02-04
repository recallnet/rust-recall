// Copyright 2025 Recall Contributors
// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

use std::fmt::Display;
use std::time::Duration;

use anyhow::{anyhow, Context};
use async_trait::async_trait;
use backoff::{backoff::Backoff, future::retry, ExponentialBackoff};
use ethers::core::types as et;
use ethers::utils::hex::ToHexExt;
use fendermint_eth_api::conv::from_tm::{
    to_chain_message, to_cumulative, to_eth_receipt, to_eth_transaction_response,
};
use fvm_shared::{address::Address, chainid::ChainID};
use reqwest::multipart::Form;
use tendermint::{abci::response::DeliverTx, block::Height, hash::Hash};
use tendermint_rpc::{
    endpoint::abci_query::AbciQuery, endpoint::block_results, Client, Scheme, WebSocketClient,
    WebSocketClientDriver, WebSocketClientUrl,
};

pub use tendermint_rpc::{HttpClient, Url};

use crate::message::{serialize, ChainMessage};
use crate::object::{NodeAddr, ObjectProvider, UploadResponse};
use crate::query::{FvmQuery, FvmQueryHeight, QueryProvider};
use crate::tx::{BroadcastMode, TxProvider, TxResult};
use crate::{Provider, TendermintClient};

/// Creates a new backoff policy.
fn new_backoff_policy(max_elapsed_secs: u64) -> ExponentialBackoff {
    let mut eb = ExponentialBackoff {
        max_elapsed_time: Some(Duration::from_secs(max_elapsed_secs)),
        ..Default::default()
    };
    eb.reset();
    eb
}

/// A JSON RPC Recall chain provider.
#[derive(Clone)]
pub struct JsonRpcProvider<C = HttpClient> {
    inner: C,
    chain_id: ChainID,
    objects: Option<ObjectClient>,
}

#[derive(Clone)]
struct ObjectClient {
    inner: reqwest::Client,
    url: Url,
}

impl JsonRpcProvider<HttpClient> {
    pub fn new_http(
        url: Url,
        chain_id: ChainID,
        proxy_url: Option<Url>,
        object_url: Option<Url>,
    ) -> anyhow::Result<Self> {
        let inner = http_client(url, proxy_url)?;
        let objects = object_url.map(|url| ObjectClient {
            inner: reqwest::Client::new(),
            url,
        });
        Ok(Self {
            inner,
            chain_id,
            objects,
        })
    }
}

impl<C> Provider<C> for JsonRpcProvider<C> where C: Client + Send + Sync {}

impl<C> TendermintClient<C> for JsonRpcProvider<C>
where
    C: Client + Send + Sync,
{
    fn underlying(&self) -> &C {
        &self.inner
    }
}

#[async_trait]
impl<C> QueryProvider for JsonRpcProvider<C>
where
    C: Client + Sync + Send,
{
    async fn query(&self, query: FvmQuery, height: FvmQueryHeight) -> anyhow::Result<AbciQuery> {
        let data = fvm_ipld_encoding::to_vec(&query).context("failed to encode query")?;
        let height: u64 = height.into();
        let height = Height::try_from(height).context("failed to conver to Height")?;
        let res = self
            .inner
            .abci_query(None, data, Some(height), false)
            .await?;
        Ok(res)
    }
}

#[async_trait]
impl<C> TxProvider for JsonRpcProvider<C>
where
    C: Client + Sync + Send,
{
    async fn perform<F, T>(
        &self,
        message: ChainMessage,
        broadcast_mode: BroadcastMode,
        f: F,
    ) -> anyhow::Result<TxResult<T>>
    where
        F: FnOnce(&DeliverTx) -> anyhow::Result<T> + Sync + Send,
        T: Sync + Send,
    {
        let data = serialize(&message)?;

        match broadcast_mode {
            BroadcastMode::Async | BroadcastMode::Sync => {
                // Build minimal tx from the signed message.
                let tx = if let ChainMessage::Signed(signed) = message.clone() {
                    to_eth_transaction_response(signed, self.chain_id)
                        .context("failed to convert to eth transaction")?
                } else {
                    return Err(anyhow!("message is not signed"));
                };

                if matches!(broadcast_mode, BroadcastMode::Async) {
                    self.inner.broadcast_tx_async(data).await?;
                    Ok(TxResult::pending(tx))
                } else {
                    let response = self.inner.broadcast_tx_sync(data).await?;
                    if response.code.is_err() {
                        return Err(anyhow!(format_err("", &response.log)));
                    }
                    Ok(TxResult::pending(tx))
                }
            }
            BroadcastMode::Commit => {
                let response = self.inner.broadcast_tx_commit(data).await?;
                if response.check_tx.code.is_err() {
                    return Err(anyhow!(format_err(
                        &response.check_tx.info,
                        &response.check_tx.log
                    )));
                } else if response.deliver_tx.code.is_err() {
                    return Err(anyhow!(format_err(
                        &response.deliver_tx.info,
                        &response.deliver_tx.log
                    )));
                }

                let return_data = f(&response.deliver_tx)
                    .context("error decoding data from deliver_tx in commit")?;

                let receipt = self.eth_tx_receipt(response.hash, false).await?;

                Ok(TxResult::committed(receipt, Some(return_data)))
            }
        }
    }

    async fn eth_tx_receipt(
        &self,
        hash: Hash,
        prove: bool,
    ) -> anyhow::Result<et::TransactionReceipt> {
        // Get tx and block header using backoff because they do not immediately show up
        // in the indexer.
        let tx_res = retry(new_backoff_policy(10), || async {
            self.inner.tx(hash, prove).await.map_err(|e| {
                backoff::Error::transient(anyhow!(
                    "cometbft transaction not found (tx_hash={}): {}",
                    hash.encode_hex_with_prefix(),
                    e
                ))
            })
        })
        .await?;
        let header = retry(new_backoff_policy(10), || async {
            self.inner.header(tx_res.height).await.map_err(|e| {
                backoff::Error::transient(anyhow!(
                    "transaction block header not found (tx_hash={}): {}",
                    hash.encode_hex_with_prefix(),
                    e
                ))
            })
        })
        .await?;

        // Header is found, block results are expected to be present, raise error is not found
        let block_results: block_results::Response =
            self.inner.block_results(tx_res.height).await?;
        let cumulative = to_cumulative(&block_results);
        let state_params = self
            .state_params(FvmQueryHeight::Height(header.header.height.value()))
            .await?;
        let msg = to_chain_message(&tx_res.tx)?;
        if let ChainMessage::Signed(msg) = msg {
            let receipt = to_eth_receipt(
                &msg,
                &tx_res,
                &cumulative,
                &header.header,
                &state_params.value.base_fee,
            )
            .await
            .context("failed to convert to receipt")?;

            Ok(receipt)
        } else {
            Err(anyhow!(
                "transaction is not convertible to Ethereum (tx_hash={})",
                hash.encode_hex_with_prefix()
            ))
        }
    }
}

#[async_trait]
impl<C> ObjectProvider for JsonRpcProvider<C>
where
    C: Client + Sync + Send,
{
    async fn node_addr(&self) -> anyhow::Result<NodeAddr> {
        let client = self
            .objects
            .clone()
            .ok_or_else(|| anyhow!("object provider is required"))?;

        let url = format!("{}v1/node", client.url);
        let response = client.inner.get(url).send().await?;
        if !response.status().is_success() {
            return Err(anyhow!(format!(
                "failed to get node address info: {}",
                response.text().await?
            )));
        }

        let addr = response.json::<NodeAddr>().await?;
        Ok(addr)
    }

    async fn upload(&self, body: reqwest::Body, size: u64) -> anyhow::Result<UploadResponse> {
        let client = self
            .objects
            .clone()
            .ok_or_else(|| anyhow!("object provider is required"))?;

        let url = format!("{}v1/objects", client.url);
        let form = Form::new().text("size", size.to_string()).part(
            "data",
            reqwest::multipart::Part::stream_with_length(body, size)
                .mime_str("application/octet-stream")?,
        );

        let response = client.inner.post(url).multipart(form).send().await?;
        if !response.status().is_success() {
            return Err(anyhow!(format!(
                "failed to upload object: {}",
                response.text().await?
            )));
        }
        let upload_response: UploadResponse = response.json().await?;
        Ok(upload_response)
    }

    async fn download(
        &self,
        address: Address,
        key: &str,
        range: Option<String>,
        height: u64,
    ) -> anyhow::Result<reqwest::Response> {
        let client = self
            .objects
            .clone()
            .ok_or_else(|| anyhow!("object provider is required"))?;

        let url = format!(
            "{}v1/objects/{}/{}?height={}",
            client.url, address, key, height
        );
        let response = if let Some(range) = range {
            client
                .inner
                .get(url)
                .header("Range", format!("bytes={}", range))
                .send()
                .await?
        } else {
            client.inner.get(url).send().await?
        };
        if !response.status().is_success() {
            return Err(anyhow!(format!(
                "failed to download object: {}",
                response.text().await?
            )));
        }

        Ok(response)
    }

    async fn size(&self, address: Address, key: &str, height: u64) -> anyhow::Result<u64> {
        let client = self
            .objects
            .clone()
            .ok_or_else(|| anyhow!("object provider is required"))?;

        let url = format!(
            "{}v1/objects/{}/{}?height={}",
            client.url, address, key, height
        );
        let response = client.inner.head(url).send().await?;
        if !response.status().is_success() {
            return Err(anyhow!(format!(
                "failed to get object size: {}",
                response.text().await?
            )));
        }

        let size: u64 = response
            .headers()
            .get("content-length")
            .ok_or_else(|| anyhow!("missing content-length header in response for object size"))?
            .to_str()?
            .parse()?;
        Ok(size)
    }
}

/// Format transaction receipt errors.
fn format_err(info: &str, log: &str) -> String {
    let mut output = String::new();
    if !info.is_empty() {
        output.push_str(info);
    }
    if !log.is_empty() {
        if !output.is_empty() {
            output.push_str("\n\n");
        }
        output.push_str(log);
    }
    output
}

// Retrieve the proxy URL with precedence:
// 1. If supplied, that's the proxy URL used.
// 2. If not supplied, but environment variable HTTP_PROXY or HTTPS_PROXY are
//    supplied, then use the appropriate variable for the URL in question.
//
// Copied from `tendermint_rpc`.
fn get_http_proxy_url(url_scheme: Scheme, proxy_url: Option<Url>) -> anyhow::Result<Option<Url>> {
    match proxy_url {
        Some(u) => Ok(Some(u)),
        None => match url_scheme {
            Scheme::Http => std::env::var("HTTP_PROXY").ok(),
            Scheme::Https => std::env::var("HTTPS_PROXY")
                .ok()
                .or_else(|| std::env::var("HTTP_PROXY").ok()),
            _ => {
                if std::env::var("HTTP_PROXY").is_ok() || std::env::var("HTTPS_PROXY").is_ok() {
                    tracing::warn!(
                        "Ignoring HTTP proxy environment variables for non-HTTP client connection"
                    );
                }
                None
            }
        }
        .map(|u| u.parse::<Url>().map_err(|e| anyhow!(e)))
        .transpose(),
    }
}

/// Create a Tendermint HTTP client.
pub fn http_client(url: Url, proxy_url: Option<Url>) -> anyhow::Result<HttpClient> {
    let proxy_url = get_http_proxy_url(url.scheme(), proxy_url)?;
    let client = match proxy_url {
        Some(proxy_url) => {
            tracing::debug!(
                "Using HTTP client with proxy {} to submit request to {}",
                proxy_url,
                url
            );
            HttpClient::new_with_proxy(url, proxy_url)?
        }
        None => {
            tracing::debug!("Using HTTP client to submit request to: {}", url);
            HttpClient::new(url)?
        }
    };
    Ok(client)
}

/// Create a Tendermint WebSocket client.
///
/// The caller must start the driver in a background task.
pub async fn ws_client<U>(url: U) -> anyhow::Result<(WebSocketClient, WebSocketClientDriver)>
where
    U: TryInto<WebSocketClientUrl, Error = tendermint_rpc::Error> + Display + Clone,
{
    // TODO: Doesn't handle proxy.
    tracing::debug!("Using WS client to submit request to: {}", url);
    let (client, driver) = WebSocketClient::new(url.clone())
        .await
        .with_context(|| format!("failed to create WS client to: {}", url))?;
    Ok((client, driver))
}
