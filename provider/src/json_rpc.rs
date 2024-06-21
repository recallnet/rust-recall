// Copyright 2024 ADM Contributors
// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

use std::fmt::Display;

use anyhow::{anyhow, Context};
use async_trait::async_trait;
use fendermint_vm_message::{
    chain::ChainMessage,
    query::{FvmQuery, FvmQueryHeight},
};
use fvm_shared::address::Address;
use iroh::net::NodeId;
use reqwest::multipart::Form;
use tendermint::abci::response::DeliverTx;
use tendermint::block::Height;
use tendermint_rpc::{
    endpoint::abci_query::AbciQuery, Client, HttpClient, Scheme, Url, WebSocketClient,
    WebSocketClientDriver, WebSocketClientUrl,
};

use crate::object::ObjectProvider;
use crate::query::QueryProvider;
use crate::response::Cid;
use crate::tx::{BroadcastMode, TxProvider, TxReceipt};
use crate::{Provider, TendermintClient};

/// A JSON RPC ADM chain provider.
#[derive(Clone)]
pub struct JsonRpcProvider<C = HttpClient> {
    inner: C,
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
        proxy_url: Option<Url>,
        object_url: Option<Url>,
    ) -> anyhow::Result<Self> {
        let inner = http_client(url, proxy_url)?;
        let objects = object_url.map(|url| ObjectClient {
            inner: reqwest::Client::new(),
            url,
        });
        Ok(Self { inner, objects })
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
    ) -> anyhow::Result<TxReceipt<T>>
    where
        F: FnOnce(&DeliverTx) -> anyhow::Result<T> + Sync + Send,
        T: Sync + Send,
    {
        match broadcast_mode {
            BroadcastMode::Async => {
                let data = crate::message::serialize(&message)?;
                let response = self.inner.broadcast_tx_async(data).await?;

                Ok(TxReceipt::pending(response.hash))
            }
            BroadcastMode::Sync => {
                let data = crate::message::serialize(&message)?;
                let response = self.inner.broadcast_tx_sync(data).await?;
                if response.code.is_err() {
                    return Err(anyhow!(response.log));
                }
                Ok(TxReceipt::pending(response.hash))
            }
            BroadcastMode::Commit => {
                let data = crate::message::serialize(&message)?;
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

                Ok(TxReceipt::committed(
                    response.hash,
                    response.height,
                    response.deliver_tx.gas_used,
                    Some(return_data),
                ))
            }
        }
    }
}

#[async_trait]
impl<C> ObjectProvider for JsonRpcProvider<C>
where
    C: Client + Sync + Send,
{
    async fn upload(
        &self,
        cid: Cid,
        source: NodeId,
        _total_bytes: usize,
        msg: String,
        chain_id: u64,
    ) -> anyhow::Result<()> {
        let client = self
            .objects
            .clone()
            .ok_or_else(|| anyhow!("object provider is required"))?;

        let form = Form::new()
            .text("chain_id", chain_id.to_string())
            .text("msg", msg)
            .text("cid", cid.to_string())
            .text("source", source.to_string());

        let url = format!("{}v1/objects", client.url);
        let response = client.inner.post(url).multipart(form).send().await?;
        if !response.status().is_success() {
            return Err(anyhow!(format!(
                "failed to upload object: {}",
                response.text().await?
            )));
        }

        Ok(())
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

    async fn size(&self, address: Address, key: &str, height: u64) -> anyhow::Result<usize> {
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

        let size: usize = response
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
    if log.is_empty() {
        info.into()
    } else {
        format!("info: {}; log: {}", info, log)
    }
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
