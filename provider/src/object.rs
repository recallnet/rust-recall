// Copyright 2024 Hoku Contributors
// SPDX-License-Identifier: Apache-2.0, MIT

use async_trait::async_trait;
use fvm_shared::address::Address;
pub use iroh::net::NodeAddr;
use reqwest::multipart::Form;
use serde::Deserialize;

/// Provider for object interactions.
#[async_trait]
pub trait ObjectProvider: Send + Sync {
    /// Get Iroh [`NodeAddr`].
    async fn node_addr(&self) -> anyhow::Result<NodeAddr>;

    /// Upload an object using multipart form data.
    async fn upload(&self, body: reqwest::Body, size: u64) -> anyhow::Result<UploadResponse>;

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

#[derive(Deserialize)]
pub struct UploadResponse {
    pub hash: String,
    pub metadata_hash: String,
}
