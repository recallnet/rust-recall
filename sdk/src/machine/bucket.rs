// Copyright 2024 Hoku Contributors
// SPDX-License-Identifier: Apache-2.0, MIT

use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::{cmp::min, collections::HashMap};

use anyhow::anyhow;
use async_trait::async_trait;
use base64::{engine::general_purpose, Engine};
use fendermint_actor_blobs_shared::state::{Hash, PublicKey};
use fendermint_actor_bucket::{
    AddParams, DeleteParams, GetParams, ListObjectsReturn, ListParams,
    Method::{AddObject, DeleteObject, GetObject, ListObjects, UpdateObjectMetadata},
    UpdateObjectMetadataParams, MAX_METADATA_KEY_SIZE, MAX_METADATA_VALUE_SIZE,
};
use fendermint_vm_actor_interface::adm::Kind;
use indicatif::{HumanDuration, MultiProgress, ProgressBar};
use infer::Type;
use iroh::blobs::{provider::AddProgress, util::SetTagOption, Hash as IrohHash};
use iroh::client::blobs::WrapOption;
use iroh::net::NodeId;
use num_traits::Zero;
use peekable::tokio::AsyncPeekable;
use serde::{Deserialize, Serialize};
use tendermint::abci::response::DeliverTx;
use tokio::sync::{mpsc, Mutex};
use tokio::{
    io::{AsyncRead, AsyncWrite, AsyncWriteExt},
    time::Instant,
};
use tokio_stream::StreamExt;

use hoku_provider::{
    fvm_ipld_encoding,
    fvm_ipld_encoding::RawBytes,
    fvm_shared::{address::Address, clock::ChainEpoch, econ::TokenAmount},
    message::{local_message, object_upload_message, GasParams},
    object::ObjectProvider,
    query::{FvmQueryHeight, QueryProvider},
    response::{decode_as, decode_bytes},
    tx::{BroadcastMode, TxReceipt},
    Client, Provider,
};
use hoku_signer::Signer;

use crate::progress::{new_message_bar, new_multi_bar, SPARKLE};
use crate::{
    machine::{deploy_machine, DeployTxReceipt, Machine},
    progress::new_progress_bar,
};

pub use fendermint_actor_bucket::{Object, ObjectState};

/// Maximum allowed object size in bytes.
const MAX_OBJECT_LENGTH: u64 = 5_000_000_000; // 5GB

/// Object add options.
#[derive(Clone, Default, Debug)]
pub struct AddOptions {
    /// Object time-to-live (TTL) duration.
    /// If a TTL is specified, credits will be reserved for the duration,
    /// after which the object will be deleted.
    /// If a TTL is not specified, the object will be continuously renewed about every hour.
    /// If the owner's free credit balance is exhausted, the object will be deleted.
    pub ttl: Option<ChainEpoch>,
    /// Metadata to add to the object.
    pub metadata: HashMap<String, String>,
    /// Overwrite the object if it already exists.
    pub overwrite: bool,
    /// Tokens to use for inline buying of credits
    pub token_amount: Option<TokenAmount>,
    /// Broadcast mode for the transaction.
    pub broadcast_mode: BroadcastMode,
    /// Gas params for the transaction.
    pub gas_params: GasParams,
    /// Whether to show progress-related output (useful for command-line interfaces).
    pub show_progress: bool,
}

/// Object delete options.
#[derive(Clone, Default, Debug)]
pub struct DeleteOptions {
    /// Broadcast mode for the transaction.
    pub broadcast_mode: BroadcastMode,
    /// Gas params for the transaction.
    pub gas_params: GasParams,
}

/// Update object metadata options.
#[derive(Clone, Default, Debug)]
pub struct UpdateObjectMetadataOptions {
    /// Broadcast mode for the transaction.
    pub broadcast_mode: BroadcastMode,
    /// Gas params for the transaction.
    pub gas_params: GasParams,
}

/// Object get options.
#[derive(Clone, Default, Debug)]
pub struct GetOptions {
    /// Optional range of bytes to get from the object.
    /// Format: "start-end" (inclusive).
    /// Example: "0-99" (first 100 bytes).
    /// This follows the HTTP range header format:
    /// `<https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Range>`
    pub range: Option<String>,
    /// Query block height.
    pub height: FvmQueryHeight,
    /// Whether to show progress-related output (useful for command-line interfaces).
    pub show_progress: bool,
}

/// Object query options.
#[derive(Clone, Debug)]
pub struct QueryOptions {
    /// The prefix to filter objects by.
    pub prefix: String,
    /// The delimiter used to define object hierarchy.
    pub delimiter: String,
    /// The key to start listing objects from.
    pub start_key: Option<Vec<u8>>,
    /// The maximum number of objects to list.
    pub limit: u64,
    /// Query block height.
    pub height: FvmQueryHeight,
}

impl Default for QueryOptions {
    fn default() -> Self {
        QueryOptions {
            prefix: Default::default(),
            delimiter: "/".into(),
            start_key: Default::default(),
            limit: Default::default(),
            height: Default::default(),
        }
    }
}

/// A machine for S3-like object storage.
pub struct Bucket {
    address: Address,
    /// The temporary root dir for the iroh node.
    /// Kept around so it is only deleted when the store gets removed.
    #[allow(dead_code)]
    iroh_dir: async_tempfile::TempDir,
    /// The iroh node, used to transfer data.
    iroh: iroh::node::FsNode,
    /// Handle to blob transfer related events from the iroh node.
    iroh_blob_events_handle: BlobEventsHandle,
}

#[async_trait]
impl Machine for Bucket {
    const KIND: Kind = Kind::Bucket;

    async fn new<C>(
        provider: &impl Provider<C>,
        signer: &mut impl Signer,
        owner: Option<Address>,

        metadata: HashMap<String, String>,
        gas_params: GasParams,
    ) -> anyhow::Result<(Self, DeployTxReceipt)>
    where
        C: Client + Send + Sync,
    {
        let (address, tx) =
            deploy_machine(provider, signer, owner, Kind::Bucket, metadata, gas_params).await?;
        let this = Self::attach(address).await?;
        Ok((this, tx))
    }

    async fn attach(address: Address) -> anyhow::Result<Self> {
        let (node_events, iroh_blob_events_handle) = BlobEvents::new(16);
        let iroh_dir = async_tempfile::TempDir::new().await?;

        let node = iroh::node::Node::persistent(iroh_dir.dir_path())
            .await?
            .blobs_events(node_events)
            .spawn()
            .await?;

        Ok(Bucket {
            address,
            iroh_dir,
            iroh: node,
            iroh_blob_events_handle,
        })
    }

    fn address(&self) -> Address {
        self.address
    }
}

#[derive(Debug, Clone)]
struct BlobEvents {
    sender: mpsc::Sender<iroh::blobs::provider::Event>,
    collect_events: Arc<AtomicBool>,
}

struct BlobEventsHandle {
    receiver: Arc<Mutex<mpsc::Receiver<iroh::blobs::provider::Event>>>,
    collect_events: Arc<AtomicBool>,
}

impl BlobEvents {
    fn new(cap: usize) -> (Self, BlobEventsHandle) {
        let (s, r) = mpsc::channel(cap);
        let collect_events = Arc::new(AtomicBool::new(false));
        (
            Self {
                sender: s,
                collect_events: collect_events.clone(),
            },
            BlobEventsHandle {
                receiver: Arc::new(Mutex::new(r)),
                collect_events,
            },
        )
    }
}

impl iroh::blobs::provider::CustomEventSender for BlobEvents {
    fn send(
        &self,
        event: iroh::blobs::provider::Event,
    ) -> Pin<Box<dyn Future<Output = ()> + 'static + Send>> {
        let sender = self.sender.clone();
        let collect_events = self.collect_events.clone();
        Box::pin(async move {
            if collect_events.load(Ordering::Relaxed) {
                sender.send(event).await.ok();
            }
        })
    }

    fn try_send(&self, event: iroh::blobs::provider::Event) {
        if self.collect_events.load(Ordering::Relaxed) {
            self.sender.try_send(event).ok();
        }
    }
}

#[derive(Deserialize)]
struct UploadResponse {
    metadata_hash: String,
}

#[derive(Serialize)]
pub struct UploadParams {
    /// Source Iroh node ID used for ingestion.
    pub source: PublicKey,
    /// Object blake3 hash.
    pub hash: Hash,
    /// Object size.
    pub size: u64,
}

impl Bucket {
    /// Add an object into the bucket with a reader.
    ///
    /// Use [`Bucket::add_from_path`] for files.
    pub async fn add_reader<C, R>(
        &self,
        provider: &impl Provider<C>,
        signer: &mut impl Signer,
        key: &str,
        reader: R,
        options: AddOptions,
    ) -> anyhow::Result<TxReceipt<Object>>
    where
        C: Client + Send + Sync,
        R: AsyncRead + Unpin + Send + 'static,
    {
        let mut reader = AsyncPeekable::from(reader);
        let mut buffer = [0u8; 40]; // 40 bytes is enough to detect mime type
        reader.peek(&mut buffer).await?;

        let content_type = infer::get(&buffer[..]);
        let options = self.add_content_type_to_metadata(options, content_type);

        let started = Instant::now();
        let bars = new_multi_bar(!options.show_progress);
        let msg_bar = bars.add(new_message_bar());

        let progress = self
            .iroh
            .blobs()
            .add_reader(reader, SetTagOption::Auto)
            .await?;

        self.add_inner(
            provider, signer, key, options, started, bars, msg_bar, progress,
        )
        .await
    }

    /// Add an object into the bucket from a path.
    pub async fn add_from_path<C>(
        &self,
        provider: &impl Provider<C>,
        signer: &mut impl Signer,
        key: &str,
        path: impl AsRef<Path>,
        options: AddOptions,
    ) -> anyhow::Result<TxReceipt<Object>>
    where
        C: Client + Send + Sync,
    {
        let path = path
            .as_ref()
            .canonicalize()
            .map_err(|e| anyhow!("failed to resolve path: {}", e))?;
        let md = tokio::fs::metadata(&path).await?;
        if !md.is_file() {
            return Err(anyhow!("input must be a file"));
        }
        if md.len() > MAX_OBJECT_LENGTH {
            return Err(anyhow!("file exceeds maximum allowed size of 5 GB"));
        }

        let content_type = infer::get_from_path(&path)?;
        let options = self.add_content_type_to_metadata(options, content_type);

        let started = Instant::now();
        let bars = new_multi_bar(!options.show_progress);
        let msg_bar = bars.add(new_message_bar());

        let progress = self
            .iroh
            .blobs()
            .add_from_path(path, true, SetTagOption::Auto, WrapOption::NoWrap)
            .await?;

        self.add_inner(
            provider, signer, key, options, started, bars, msg_bar, progress,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn add_inner<C>(
        &self,
        provider: &impl Provider<C>,
        signer: &mut impl Signer,
        key: &str,
        options: AddOptions,
        started: Instant,
        bars: Arc<MultiProgress>,
        msg_bar: ProgressBar,
        mut progress: iroh::client::blobs::AddProgress,
    ) -> anyhow::Result<TxReceipt<Object>>
    where
        C: Client + Send + Sync,
    {
        validate_metadata(&options.metadata)?;

        // Iroh ingest
        msg_bar.set_prefix("[1/3]");
        msg_bar.set_message("Ingesting data ...");

        let pro_bar = bars.add(new_progress_bar(0));
        let mut object_size = 0;
        pro_bar.set_position(0);

        let object_hash = loop {
            let Some(event) = progress.next().await else {
                anyhow::bail!("Unexpected end while ingesting data");
            };
            match event? {
                AddProgress::Found {
                    id: _,
                    name: _,
                    size,
                } => {
                    object_size = size;
                    pro_bar.set_length(size);
                }
                AddProgress::Done { id: _, hash: _ } => {
                    pro_bar.finish_and_clear();
                }
                AddProgress::AllDone { hash, .. } => {
                    break hash;
                }
                AddProgress::Progress { id: _, offset } => {
                    pro_bar.set_position(offset);
                }
                AddProgress::Abort(err) => {
                    return Err(err.into());
                }
            }
        };

        // Upload
        msg_bar.set_prefix("[2/3]");
        msg_bar.set_message(format!("Uploading {} to network...", object_hash));

        let node_addr = provider.node_addr().await?;
        let up_bar = bars.add(new_progress_bar(object_size));

        // Start collecting events for progress
        self.iroh_blob_events_handle
            .collect_events
            .store(true, Ordering::Relaxed);
        let r = self.iroh_blob_events_handle.receiver.clone();
        let (cancel_s, mut cancel_r) = tokio::sync::oneshot::channel();

        tokio::task::spawn(async move {
            let mut r = r.lock().await;
            let mut current_req = None;
            up_bar.set_position(0);

            loop {
                tokio::select! {
                    _ = &mut cancel_r => {
                        // finished
                        break;
                    }
                    Some(event) = r.recv() => {
                        match event {
                            iroh::blobs::provider::Event::GetRequestReceived {
                                request_id, hash, ..
                            } => {
                                if hash == object_hash {
                                    current_req.replace(request_id);
                                }
                            }
                            iroh::blobs::provider::Event::TransferProgress {
                                request_id,
                                hash,
                                end_offset,
                                ..
                            } => {
                                if hash == object_hash && Some(request_id) == current_req {
                                    // progress
                                    up_bar.set_position(end_offset);
                                }
                            }
                            iroh::blobs::provider::Event::TransferCompleted { request_id, .. } => {
                                if Some(request_id) == current_req {
                                    break;
                                }
                            }
                            iroh::blobs::provider::Event::TransferAborted { request_id, .. } => {
                                if Some(request_id) == current_req {
                                    break;
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }

            up_bar.finish_and_clear();
        });

        let metadata_hash = self
            .upload(
                provider,
                node_addr.node_id,
                signer,
                object_hash,
                object_size,
            )
            .await?;

        let metadata_hash = IrohHash::from_str(metadata_hash.as_str())?;

        cancel_s.send(()).ok();
        self.iroh_blob_events_handle
            .collect_events
            .store(false, Ordering::Relaxed);

        // Broadcast transaction with Object
        msg_bar.set_prefix("[3/3]");
        msg_bar.set_message("Broadcasting transaction...");
        let params = AddParams {
            source: fendermint_actor_blobs_shared::state::PublicKey(*node_addr.node_id.as_bytes()),
            key: key.into(),
            hash: fendermint_actor_blobs_shared::state::Hash(*object_hash.as_bytes()),
            recovery_hash: fendermint_actor_blobs_shared::state::Hash(*metadata_hash.as_bytes()),
            size: object_size,
            ttl: options.ttl,
            metadata: options.metadata,
            overwrite: options.overwrite,
        };
        let serialized_params = RawBytes::serialize(params.clone())?;
        let token_amount = options.token_amount.unwrap_or(TokenAmount::zero());
        let message = signer
            .transaction(
                self.address,
                token_amount,
                AddObject as u64,
                serialized_params,
                options.gas_params,
            )
            .await?;

        let tx = provider
            .perform(message, options.broadcast_mode, decode_as)
            .await?;

        msg_bar.println(format!(
            "{} Added object in {} (hash={}; size={})",
            SPARKLE,
            HumanDuration(started.elapsed()),
            object_hash,
            object_size
        ));

        msg_bar.finish_and_clear();
        Ok(tx)
    }

    /// Uploads an object to the Object API for staging.
    #[allow(clippy::too_many_arguments)]
    async fn upload(
        &self,
        provider: &impl ObjectProvider,
        provider_node_id: NodeId,
        signer: &mut impl Signer,
        hash: IrohHash,
        size: u64,
    ) -> anyhow::Result<String> {
        let from = signer.address();
        let params = UploadParams {
            source: fendermint_actor_blobs_shared::state::PublicKey(*provider_node_id.as_bytes()),
            hash: fendermint_actor_blobs_shared::state::Hash(*hash.as_bytes()),
            size,
        };
        let serialized_params = RawBytes::serialize(params)?;

        let message =
            object_upload_message(from, self.address, AddObject as u64, serialized_params);
        let singed_message = signer.sign_message(message)?;
        let serialized_signed_message = fvm_ipld_encoding::to_vec(&singed_message)?;

        let chain_id = match signer.subnet_id() {
            Some(id) => id.chain_id(),
            None => {
                return Err(anyhow!("failed to get subnet ID from signer"));
            }
        };

        let node_addr = self.iroh.net().node_addr().await?;
        let response = provider
            .upload(
                hash,
                node_addr,
                size,
                general_purpose::URL_SAFE.encode(&serialized_signed_message),
                chain_id.into(),
            )
            .await?;

        let response = response.json::<UploadResponse>().await?;

        Ok(response.metadata_hash)
    }

    /// Delete an object.
    pub async fn delete<C>(
        &self,
        provider: &impl Provider<C>,
        signer: &mut impl Signer,
        key: &str,
        options: DeleteOptions,
    ) -> anyhow::Result<TxReceipt<()>>
    where
        C: Client + Send + Sync,
    {
        let params = DeleteParams(key.into());
        let params = RawBytes::serialize(params)?;
        let message = signer
            .transaction(
                self.address,
                Default::default(),
                DeleteObject as u64,
                params,
                options.gas_params,
            )
            .await?;
        provider
            .perform(
                message,
                options.broadcast_mode,
                |_: &DeliverTx| -> anyhow::Result<()> { Ok(()) },
            )
            .await
    }

    /// Get an object at the given key, range, and height.
    pub async fn get<W>(
        &self,
        provider: &(impl QueryProvider + ObjectProvider),
        key: &str,
        mut writer: W,
        options: GetOptions,
    ) -> anyhow::Result<()>
    where
        W: AsyncWrite + Unpin + Send + 'static,
    {
        let started = Instant::now();
        let bars = new_multi_bar(!options.show_progress);
        let msg_bar = bars.add(new_message_bar());

        msg_bar.set_prefix("[1/2]");
        msg_bar.set_message("Getting object info...");
        let params = GetParams(key.into());
        let params = RawBytes::serialize(params)?;
        let message = local_message(self.address, GetObject as u64, params);
        let response = provider.call(message, options.height, decode_get).await?;
        let object = response
            .value
            .ok_or_else(|| anyhow!("object not found for key '{}'", key))?;

        msg_bar.set_prefix("[2/2]");
        msg_bar.set_message(format!(
            "Downloading object (hash={}; size={})",
            object.hash, object.size
        ));

        let object_size = provider
            .size(self.address, key, options.height.into())
            .await?;
        let pro_bar = bars.add(new_progress_bar(object_size));
        let response = provider
            .download(self.address, key, options.range, options.height.into())
            .await?;
        let mut stream = response.bytes_stream();
        let mut progress = 0;
        while let Some(item) = stream.next().await {
            match item {
                Ok(chunk) => {
                    writer.write_all(&chunk).await?;
                    progress = min(progress + chunk.len(), object_size as usize);
                    pro_bar.set_position(progress as u64);
                }
                Err(e) => {
                    return Err(anyhow!(e));
                }
            }
        }
        pro_bar.finish_and_clear();
        msg_bar.println(format!(
            "{} Downloaded object in {} (hash={}; size={})",
            SPARKLE,
            HumanDuration(started.elapsed()),
            object.hash,
            object.size
        ));

        msg_bar.finish_and_clear();
        Ok(())
    }

    /// Query for objects with params at the given height.
    ///
    /// Use [`QueryOptions`] for filtering and pagination.
    pub async fn query(
        &self,
        provider: &impl QueryProvider,
        options: QueryOptions,
    ) -> anyhow::Result<ListObjectsReturn> {
        let params = ListParams {
            prefix: options.prefix.into(),
            delimiter: options.delimiter.into(),
            start_key: options.start_key,
            limit: options.limit,
        };
        let params = RawBytes::serialize(params)?;
        let message = local_message(self.address, ListObjects as u64, params);
        let response = provider.call(message, options.height, decode_list).await?;
        Ok(response.value)
    }

    /// Update object metadata.
    ///
    /// New metadata gets added, and existing gets updated, and empty value metadata gets deleted.
    pub async fn update_object_metadata<C>(
        &self,
        provider: &impl Provider<C>,
        signer: &mut impl Signer,
        key: &str,
        metadata: HashMap<String, Option<String>>,
        options: UpdateObjectMetadataOptions,
    ) -> anyhow::Result<TxReceipt<()>>
    where
        C: Client + Send + Sync,
    {
        validate_metadata_optional(&metadata)?;

        let params = UpdateObjectMetadataParams {
            key: key.into(),
            metadata,
        };
        let params = RawBytes::serialize(params)?;
        let message = signer
            .transaction(
                self.address,
                Default::default(),
                UpdateObjectMetadata as u64,
                params,
                options.gas_params,
            )
            .await?;
        provider
            .perform(
                message,
                options.broadcast_mode,
                |_: &DeliverTx| -> anyhow::Result<()> { Ok(()) },
            )
            .await
    }

    fn add_content_type_to_metadata(
        &self,
        options: AddOptions,
        content_type: Option<Type>,
    ) -> AddOptions {
        let mut metadata = options.metadata;
        metadata.insert(
            "content-type".into(),
            content_type.map_or("application/octet-stream".into(), |t| t.mime_type().into()),
        );

        AddOptions {
            metadata,
            ..options
        }
    }
}

fn decode_get(deliver_tx: &DeliverTx) -> anyhow::Result<Option<Object>> {
    let data = decode_bytes(deliver_tx)?;
    fvm_ipld_encoding::from_slice(&data)
        .map_err(|e| anyhow!("error parsing as Option<Object>: {e}"))
}

fn decode_list(deliver_tx: &DeliverTx) -> anyhow::Result<ListObjectsReturn> {
    let data = decode_bytes(deliver_tx)?;
    fvm_ipld_encoding::from_slice(&data)
        .map_err(|e| anyhow!("error parsing as ListObjectsReturn: {e}"))
}

fn validate_metadata(metadata: &HashMap<String, String>) -> anyhow::Result<()> {
    for (key, value) in metadata {
        if key.len() as u32 > MAX_METADATA_KEY_SIZE {
            return Err(anyhow!(
                "key must be less than or equal to {}",
                MAX_METADATA_KEY_SIZE
            ));
        }

        if value.is_empty() || value.len() as u32 > MAX_METADATA_VALUE_SIZE {
            return Err(anyhow!(
                "value must non-empty and less than or equal to {}",
                MAX_METADATA_VALUE_SIZE
            ));
        }
    }

    Ok(())
}

fn validate_metadata_optional(metadata: &HashMap<String, Option<String>>) -> anyhow::Result<()> {
    for (key, value) in metadata {
        if key.len() as u32 > MAX_METADATA_KEY_SIZE {
            return Err(anyhow!(
                "key must be less than or equal to{}",
                MAX_METADATA_KEY_SIZE
            ));
        }

        if let Some(value) = value {
            if value.is_empty() || value.len() as u32 > MAX_METADATA_VALUE_SIZE {
                return Err(anyhow!(
                    "value must non-empty and less than or equal to {}",
                    MAX_METADATA_VALUE_SIZE
                ));
            }
        }
    }

    Ok(())
}
