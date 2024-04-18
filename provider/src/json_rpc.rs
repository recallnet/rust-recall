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
use tendermint::abci::response::DeliverTx;
use tendermint::block::Height;
use tendermint_rpc::{
    endpoint::abci_query::AbciQuery, Client, HttpClient, Scheme, Url, WebSocketClient,
    WebSocketClientDriver, WebSocketClientUrl,
};

use crate::provider::{BroadcastMode, QueryProvider, Tx, TxProvider};
use crate::{Provider, TendermintClient};

#[derive(Clone)]
pub struct JsonRpcProvider<C = HttpClient> {
    inner: C,
}

impl<C> JsonRpcProvider<C> {
    pub fn new(inner: C) -> Self {
        Self { inner }
    }
}

impl JsonRpcProvider<HttpClient> {
    pub fn new_http(url: Url, proxy_url: Option<Url>) -> anyhow::Result<Self> {
        let inner = http_client(url, proxy_url)?;
        Ok(Self { inner })
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
    ) -> anyhow::Result<Tx<T>>
    where
        F: FnOnce(&DeliverTx) -> anyhow::Result<T> + Sync + Send,
        T: Sync + Send,
    {
        match broadcast_mode {
            BroadcastMode::Async => {
                let data = crate::message::serialize(&message)?;
                let response = self.inner.broadcast_tx_async(data).await?;

                Ok(Tx::pending(response.hash))
            }
            BroadcastMode::Sync => {
                let data = crate::message::serialize(&message)?;
                let response = self.inner.broadcast_tx_sync(data).await?;
                if response.code.is_err() {
                    return Err(anyhow!(response.log));
                }
                Ok(Tx::pending(response.hash))
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

                Ok(Tx::committed(
                    response.hash,
                    response.height,
                    response.deliver_tx.gas_used,
                    Some(return_data),
                ))
            }
        }
    }
}

fn format_err(info: &str, log: &str) -> String {
    format!("info: {}; log: {}", info, log)
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
