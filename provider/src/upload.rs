use anyhow::anyhow;
use async_trait::async_trait;
use cid::Cid;
use reqwest::multipart::{Form, Part};
use reqwest::Client;
use tendermint_rpc::Url;

pub struct UploadResponse {
    pub cid: Cid,
}

#[async_trait]
pub trait ObjectUploader {
    async fn upload(
        &self,
        body: reqwest::Body,
        size: usize,
        msg: String,
    ) -> anyhow::Result<UploadResponse>;
}

pub struct ObjectClient {
    inner: Client,
    endpoint: Url,
    chain_id: u64,
}

impl ObjectClient {
    pub fn new(endpoint: Url, chain_id: u64) -> Self {
        ObjectClient {
            inner: Client::new(),
            endpoint,
            chain_id,
        }
    }
}

#[async_trait]
impl ObjectUploader for ObjectClient {
    async fn upload(
        &self,
        body: reqwest::Body,
        total_bytes: usize,
        msg: String,
    ) -> anyhow::Result<UploadResponse> {
        let part = Part::stream_with_length(body, total_bytes as u64)
            .file_name("upload")
            .mime_str("application/octet-stream")?;

        let form = Form::new()
            .text("chain_id", self.chain_id.to_string())
            .text("msg", msg)
            .part("upload", part);

        let url = format!("{}v1/object", self.endpoint);
        let response = self.inner.put(url).multipart(form).send().await?;
        if !response.status().is_success() {
            return Err(anyhow!(format!(
                "failed to upload object: {}",
                response.text().await?
            )));
        }
        let cid_str = response.text().await?;
        let cid = Cid::try_from(cid_str)?;
        Ok(UploadResponse { cid })
    }
}
