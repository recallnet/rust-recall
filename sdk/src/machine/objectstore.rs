// Copyright 2024 ADM Contributors
// SPDX-License-Identifier: Apache-2.0, MIT

use std::sync::Arc;

use anyhow::anyhow;
use async_tempfile::TempFile;
use async_trait::async_trait;
use base64::{engine::general_purpose, Engine};
use bytes::Bytes;
use fendermint_actor_machine::WriteAccess;
use fendermint_actor_objectstore::{
    DeleteParams, GetParams, ListParams,
    Method::{DeleteObject, GetObject, ListObjects, PutObject},
    Object, ObjectKind, ObjectList, PutParams,
};
use fendermint_vm_actor_interface::adm::Kind;
use fendermint_vm_message::{query::FvmQueryHeight, signed::Object as MessageObject};
use fvm_ipld_encoding::RawBytes;
use fvm_shared::{address::Address, chainid::ChainID};
use tendermint::abci::response::DeliverTx;
use tendermint_rpc::Client;
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncSeekExt, AsyncWrite, AsyncWriteExt},
    sync::{mpsc, mpsc::Sender, Mutex},
    task::LocalSet,
};
use tokio_stream::{wrappers::ReceiverStream, StreamExt};
use unixfs_v1::file::adder::{Chunker, FileAdder};

use adm_provider::{
    message::{local_message, object_upload_message},
    object::ObjectService,
    response::{decode_bytes, decode_cid, Cid},
    BroadcastMode, Provider, QueryProvider, Tx,
};
use adm_signer::Signer;

use crate::machine::{deploy_machine, DeployTx, Machine};
use crate::TxArgs;

pub struct ObjectStore {
    address: Address,
}

#[async_trait]
impl Machine for ObjectStore {
    async fn new<C>(
        provider: &impl Provider<C>,
        signer: &mut impl Signer,
        write_access: WriteAccess,
        args: TxArgs,
    ) -> anyhow::Result<(Self, DeployTx)>
    where
        C: Client + Send + Sync,
    {
        let (address, tx) =
            deploy_machine(provider, signer, Kind::ObjectStore, write_access, args).await?;
        Ok((Self::attach(address), tx))
    }

    fn attach(address: Address) -> Self {
        ObjectStore { address }
    }

    fn address(&self) -> Address {
        self.address
    }
}

impl ObjectStore {
    pub async fn put<C>(
        &self,
        provider: &impl Provider<C>,
        signer: &mut impl Signer,
        params: PutParams,
        broadcast_mode: BroadcastMode,
        args: TxArgs,
    ) -> anyhow::Result<Tx<Cid>>
    where
        C: Client + Send + Sync,
    {
        let object = match &params.kind {
            ObjectKind::Internal(_) => None,
            ObjectKind::External(cid) => {
                Some(MessageObject::new(params.key.clone(), *cid, self.address))
            }
        };
        let params = RawBytes::serialize(params)?;
        let message = signer
            .transaction(
                self.address,
                Default::default(),
                PutObject as u64,
                params,
                object,
                args.gas_params,
            )
            .await?;
        provider.perform(message, broadcast_mode, decode_cid).await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn upload(
        &self,
        signer: &mut impl Signer,
        object_client: impl ObjectService,
        key: String,
        cid: cid::Cid,
        size: usize,
        overwrite: bool,
        chain_id: ChainID,
        rx: mpsc::Receiver<Vec<u8>>,
    ) -> anyhow::Result<cid::Cid> {
        let from = signer.address();
        let key = key.as_bytes().to_vec();
        let params = PutParams {
            key: key.clone(),
            kind: ObjectKind::External(cid),
            overwrite,
        };
        let serialized_params = RawBytes::serialize(params)?;
        let message =
            object_upload_message(from, self.address, PutObject as u64, serialized_params);
        let singed_message =
            signer.sign_message(message, Some(MessageObject::new(key, cid, self.address)))?;
        let serialized_signed_message = fvm_ipld_encoding::to_vec(&singed_message)?;

        let object_stream = ReceiverStream::new(rx)
            .map(|bytes_vec| Ok::<Bytes, reqwest::Error>(Bytes::from(bytes_vec)));
        let body = reqwest::Body::wrap_stream(object_stream);
        let response = object_client
            .upload(
                body,
                size,
                general_purpose::URL_SAFE.encode(&serialized_signed_message),
                chain_id.into(),
            )
            .await?;
        Ok(response.cid)
    }

    pub async fn delete<C>(
        &self,
        provider: &impl Provider<C>,
        signer: &mut impl Signer,
        params: DeleteParams,
        broadcast_mode: BroadcastMode,
        args: TxArgs,
    ) -> anyhow::Result<Tx<Cid>>
    where
        C: Client + Send + Sync,
    {
        let params = RawBytes::serialize(params)?;
        let message = signer
            .transaction(
                self.address,
                Default::default(),
                DeleteObject as u64,
                params,
                None,
                args.gas_params,
            )
            .await?;
        provider.perform(message, broadcast_mode, decode_cid).await
    }

    pub async fn get(
        &self,
        provider: &impl QueryProvider,
        params: GetParams,
        height: FvmQueryHeight,
    ) -> anyhow::Result<Option<Object>> {
        let params = RawBytes::serialize(params)?;
        let message = local_message(self.address, GetObject as u64, params);
        let response = provider.call(message, height, decode_get).await?;
        Ok(response.value)
    }

    pub async fn download(
        &self,
        object_client: impl ObjectService,
        key: String,
        writer: impl AsyncWrite + Unpin + Send + 'static,
    ) -> anyhow::Result<()> {
        object_client
            .download(self.address.to_string(), key, writer)
            .await?;
        Ok(())
    }

    pub async fn list(
        &self,
        provider: &impl QueryProvider,
        params: ListParams,
        height: FvmQueryHeight,
    ) -> anyhow::Result<Option<ObjectList>> {
        let params = RawBytes::serialize(params)?;
        let message = local_message(self.address, ListObjects as u64, params);
        let response = provider.call(message, height, decode_list).await?;
        Ok(response.value)
    }
}

fn decode_get(deliver_tx: &DeliverTx) -> anyhow::Result<Option<Object>> {
    let data = decode_bytes(deliver_tx)?;
    fvm_ipld_encoding::from_slice::<Option<Object>>(&data)
        .map_err(|e| anyhow!("error parsing as Option<Object>: {e}"))
}

fn decode_list(deliver_tx: &DeliverTx) -> anyhow::Result<Option<ObjectList>> {
    let data = decode_bytes(deliver_tx)?;
    fvm_ipld_encoding::from_slice::<Option<ObjectList>>(&data)
        .map_err(|e| anyhow!("error parsing as Option<ObjectList>: {e}"))
}

#[derive(Clone)]
struct ObjectProcessor {
    tmp: Arc<Mutex<TempFile>>,
    total_bytes: Arc<Mutex<usize>>,
}

impl ObjectProcessor {
    async fn new() -> anyhow::Result<Self> {
        let tmp = TempFile::new().await?;
        Ok(ObjectProcessor {
            tmp: Arc::new(Mutex::new(tmp)),
            total_bytes: Arc::new(Mutex::new(0)),
        })
    }
}

impl ObjectProcessor {
    async fn processed_bytes_count(&self) -> usize {
        *self.total_bytes.lock().await
    }

    async fn process_chunk(
        &mut self,
        tx: Sender<Vec<u8>>,
        first_chunk: Vec<u8>,
        mut reader: impl AsyncRead + Unpin,
    ) -> anyhow::Result<()> {
        // write first chunk to mpsc channel for uploading
        // and also to temp file for CID computation
        tx.send(first_chunk.clone()).await?;
        let mut object_file = self.tmp.lock().await;
        object_file.write_all(&first_chunk).await?;

        // read remaining bytes from the reader into temp file and mpsc channel
        let mut buffer = vec![0; 10 * 1024 * 1024];
        loop {
            match reader.read(&mut buffer).await {
                Ok(0) => {
                    break;
                }
                Ok(n) => {
                    if tx.send(buffer[..n].to_vec()).await.is_err() {
                        return Err(anyhow!("error sending data to channel"))?;
                    }
                    object_file.write_all(&buffer[..n]).await?;
                    *self.total_bytes.lock().await += n;
                }
                Err(e) => {
                    return Err(e.into());
                }
            }
        }
        object_file.flush().await?;
        object_file.rewind().await?;

        Ok(())
    }

    async fn generate_cid(&mut self) -> anyhow::Result<cid::Cid> {
        let chunk_size = 1024 * 1024; // size-1048576
        let mut adder = FileAdder::builder()
            .with_chunker(Chunker::Size(chunk_size))
            .build();
        let mut buffer = vec![0; chunk_size];
        let mut tmp = self.tmp.lock().await;
        loop {
            match tmp.read(&mut buffer).await {
                Ok(0) => {
                    break;
                }
                Ok(n) => {
                    let _ = adder.push(&buffer[..n]);
                }
                Err(e) => {
                    return Err(e.into());
                }
            }
        }
        let unixfs_iterator = adder.finish();
        let (cid, _) = unixfs_iterator
            .last()
            .ok_or_else(|| anyhow!("Cannot get root CID"))?;
        let cid = cid::Cid::try_from(cid.to_bytes())
            .map_err(|e| anyhow!("Cannot generate CID: {}", e))?;
        Ok(cid)
    }
}

/// Process the object from the reader and send it to the channel.
/// Returns the CID and the total bytes read from the reader.
///
/// Uses a LocalSet to spawn the non-Send future.
/// This is necessary because the clap's AsyncReader impl
/// is not Send. LocalSet is used to spawn the non-Send futures.
pub async fn process_object(
    reader: impl AsyncRead + Unpin + 'static,
    tx: Sender<Vec<u8>>,
    first_chunk: Vec<u8>,
) -> anyhow::Result<(cid::Cid, usize)> {
    let local_set = LocalSet::new();
    let mut object_processor = ObjectProcessor::new().await?;

    let chunk = first_chunk[..].to_vec();
    // clone and move the object_processor into the future
    let mut object_processor_clone = object_processor.clone();
    local_set.spawn_local(async move {
        object_processor_clone
            .process_chunk(tx, chunk, reader)
            .await
    });
    local_set.await;

    Ok((
        object_processor.generate_cid().await?,
        object_processor.processed_bytes_count().await,
    ))
}

#[cfg(test)]
mod tests {
    use std::io::Error;
    use std::pin::Pin;
    use std::task::{Context, Poll};

    use rand::{thread_rng, Rng};
    use tokio::io::AsyncRead;
    use tokio::io::ReadBuf;
    use tokio::sync::mpsc;

    use super::*;

    struct MockReader {
        content: Vec<u8>,
        pos: usize,
    }

    impl MockReader {
        fn new(content: Vec<u8>) -> Self {
            MockReader { content, pos: 0 }
        }
    }

    impl AsyncRead for MockReader {
        fn poll_read(
            mut self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            buf: &mut ReadBuf<'_>,
        ) -> Poll<Result<(), Error>> {
            if self.pos >= self.content.len() {
                return Poll::Ready(Ok(()));
            }
            let max_len = buf.remaining();
            let data_len = self.content.len() - self.pos;
            let len_to_copy = std::cmp::min(max_len, data_len);

            buf.put_slice(&self.content[self.pos..self.pos + len_to_copy]);
            self.pos += len_to_copy;

            Poll::Ready(Ok(()))
        }
    }

    #[tokio::test]
    async fn test_process_object() {
        let mut rng = thread_rng();
        let mut reader_content = vec![0u8; 1024];
        rng.fill(&mut reader_content[..]);

        let mut reader = MockReader::new(reader_content.clone());
        let (tx, mut rx) = mpsc::channel(10);

        // Read first 1024 bytes from the reader an assign it to first chunk
        let mut first_chunk = vec![0u8; 1024];
        reader
            .read_exact(&mut first_chunk)
            .await
            .expect("Failed to read the first chunk");

        let (cid, total) = process_object(reader, tx, first_chunk.clone())
            .await
            .unwrap();

        // Initialize an empty vector to hold the chunks received
        // from object processor
        let mut sent_chunks = Vec::new();
        while let Some(chunk) = rx.recv().await {
            sent_chunks.push(chunk);
        }

        // Verify total bytes_read (first_chunk + remaining)
        assert_eq!(total + first_chunk.len(), 1024);

        // Verify CID calculation was correct by hashing the reader
        let mut tmp = TempFile::new().await.unwrap();
        tmp.write_all(&reader_content).await.unwrap();
        tmp.flush().await.unwrap();
        tmp.rewind().await.unwrap();
        let chunk_size = 1024 * 1024;
        let mut adder = FileAdder::builder()
            .with_chunker(Chunker::Size(chunk_size))
            .build();
        let mut buffer = vec![0; chunk_size];
        loop {
            match tmp.read(&mut buffer).await {
                Ok(0) => {
                    break;
                }
                Ok(n) => {
                    let _ = adder.push(&buffer[..n]);
                }
                Err(e) => {
                    panic!("Error reading from temp file: {}", e);
                }
            }
        }
        let unixfs_iterator = adder.finish();
        let (expected_cid, _) = unixfs_iterator.last().unwrap();
        let expected_cid = cid::Cid::try_from(expected_cid.to_bytes()).unwrap();

        assert_eq!(expected_cid, cid);
    }
}
