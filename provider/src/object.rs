// Copyright 2024 ADM Contributors
// SPDX-License-Identifier: Apache-2.0, MIT

use async_trait::async_trait;
use fvm_shared::address::Address;
use iroh::blobs::Hash;
use iroh::net::NodeAddr;

/// Provider for object interactions.
#[async_trait]
pub trait ObjectProvider: Send + Sync {
    /// Get Iroh [`NodeAddr`].
    async fn node_addr(&self) -> anyhow::Result<NodeAddr>;

    /// Upload an object.
    async fn upload(
        &self,
        hash: Hash,
        source: NodeAddr,
        size: u64,
        msg: String,
        chain_id: u64,
    ) -> anyhow::Result<()>;

    /// Download an object.
    async fn download(
        &self,
        address: Address,
        key: &str,
        range: Option<String>,
        height: u64,
    ) -> anyhow::Result<reqwest::Response>;

    /// Gets the object size.
    async fn size(&self, address: Address, key: &str, height: u64) -> anyhow::Result<u64>;
}
