use anyhow::anyhow;
use async_trait::async_trait;
use cid::Cid;
use futures_util::StreamExt;
use reqwest::multipart::{Form, Part};
use reqwest::Client;
use tendermint_rpc::Url;
use tokio::io::{AsyncWrite, AsyncWriteExt};

pub struct UploadResponse {
    pub cid: Cid,
}

#[async_trait]
pub trait ObjectService {
    async fn upload(
        &self,
        body: reqwest::Body,
        size: usize,
        msg: String,
    ) -> anyhow::Result<UploadResponse>;

    async fn download(
        &self,
        address: String,
        key: String,
        writer: impl AsyncWrite + Unpin + Send + 'static,
    ) -> anyhow::Result<()>;
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
impl ObjectService for ObjectClient {
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
            .part("object", part);

        let url = format!("{}v1/objects", self.endpoint);
        let response = self.inner.post(url).multipart(form).send().await?;
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

    async fn download(
        &self,
        address: String,
        key: String,
        mut writer: impl AsyncWrite + Unpin + Send + 'static,
    ) -> anyhow::Result<()> {
        let url = format!("{}v1/objectstores/{}/{}", self.endpoint, address, key);
        let response = self.inner.get(url).send().await?;
        if !response.status().is_success() {
            return Err(anyhow!(format!(
                "failed to download object: {}",
                response.text().await?
            )));
        }

        let mut stream = response.bytes_stream();
        while let Some(item) = stream.next().await {
            match item {
                Ok(chunk) => {
                    writer.write_all(&chunk).await?;
                }
                Err(e) => {
                    return Err(anyhow!(e));
                }
            }
        }

        Ok(())
    }
}
