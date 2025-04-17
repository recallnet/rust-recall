// Copyright 2025 Recall Contributors
// SPDX-License-Identifier: Apache-2.0, MIT

use std::path::Path;
use std::{cmp::min, collections::HashMap, str::FromStr};

use anyhow::anyhow;
use async_trait::async_trait;
use fendermint_actor_blobs_shared::state::{Hash, PublicKey};
use fendermint_actor_bucket::{
    AddParams, DeleteParams, GetParams, ListObjectsReturn, ListParams,
    Method::{AddObject, DeleteObject, GetObject, ListObjects, UpdateObjectMetadata},
    UpdateObjectMetadataParams, MAX_METADATA_KEY_SIZE, MAX_METADATA_VALUE_SIZE,
};
use fendermint_vm_actor_interface::adm::{CreateExternalReturn, Kind};
use indicatif::HumanDuration;
use iroh_blobs::Hash as IrohHash;
use peekable::tokio::AsyncPeekable;
use tendermint::abci::response::DeliverTx;
use tokio::io::{AsyncRead, AsyncSeekExt, AsyncWrite, AsyncWriteExt};
use tokio::time::Instant;
use tokio_stream::StreamExt;
use tokio_util::io::ReaderStream;

use recall_provider::{
    fvm_ipld_encoding,
    fvm_ipld_encoding::RawBytes,
    fvm_shared::{address::Address, clock::ChainEpoch, econ::TokenAmount},
    message::{local_message, GasParams},
    object::ObjectProvider,
    query::{FvmQueryHeight, QueryProvider},
    response::{decode_as, decode_bytes},
    tx::{BroadcastMode, TxResult},
    Client, Provider,
};
use recall_signer::Signer;

use crate::progress::{new_message_bar, new_multi_bar, SPARKLE};
use crate::{
    machine::{deploy_machine, Machine},
    progress::new_progress_bar,
};
pub use fendermint_actor_bucket::{Object, ObjectState};

/// Maximum allowed object size in bytes.
const MAX_OBJECT_LENGTH: u64 = 5_000_000_000; // 5GB

/// Object add options.
#[derive(Clone, Default, Debug)]
pub struct AddOptions {
    /// Object time-to-live (TTL) duration.
    /// Credits will be reserved for the duration, after which the object will be deleted.
    /// If not specified, the current default TTL from the config actor is used.
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
    ) -> anyhow::Result<(Self, TxResult<CreateExternalReturn>)>
    where
        C: Client + Send + Sync,
    {
        let (address, tx) =
            deploy_machine(provider, signer, owner, Kind::Bucket, metadata, gas_params).await?;
        let this = Self::attach(address).await?;
        Ok((this, tx))
    }

    async fn attach(address: Address) -> anyhow::Result<Self> {
        Ok(Bucket { address })
    }

    fn address(&self) -> Address {
        self.address
    }
}

impl Bucket {
    /// Add an object into the bucket with a reader.
    ///
    /// Use [`Bucket::add_from_path`] for files.
    #[allow(clippy::too_many_arguments)]
    pub async fn add_reader<C, R>(
        &self,
        provider: &impl Provider<C>,
        signer: &mut impl Signer,
        from: Address,
        key: &str,
        reader: R,
        size: u64,
        options: AddOptions,
    ) -> anyhow::Result<TxResult<Object>>
    where
        C: Client + Send + Sync,
        R: AsyncRead + Unpin + Send + 'static,
    {
        let mut reader = AsyncPeekable::from(reader);
        let mut buffer = [0u8; 40]; // 40 bytes is enough to detect the mime type
        reader.peek(&mut buffer).await?;
        let content_type = infer::get(&buffer[..]).map(|t| t.to_string());

        validate_metadata(&options.metadata)?;
        let options = self.add_content_type_to_metadata(options, content_type);

        let started = Instant::now();
        let bars = new_multi_bar(!options.show_progress);
        let msg_bar = bars.add(new_message_bar());
        let pro_bar = bars.add(new_progress_bar(size));
        let upload_progress = pro_bar.clone();

        msg_bar.set_prefix("[1/2]");
        msg_bar.set_message("Starting upload to server...");

        let stream = ReaderStream::with_capacity(reader, 64 * 1024).map(move |result| {
            result.inspect(|chunk| {
                upload_progress.inc(chunk.len() as u64);
            })
        });

        let upload_response = provider
            .upload(reqwest::Body::wrap_stream(stream), size)
            .await?;

        pro_bar.finish_and_clear();
        msg_bar.set_message("Upload completed, processing response...");

        let metadata_hash = IrohHash::from_str(&upload_response.metadata_hash)
            .map_err(|_| anyhow!("Invalid metadata hash from server"))?;
        let object_hash = IrohHash::from_str(&upload_response.hash)
            .map_err(|_| anyhow!("Invalid object hash from server"))?;

        msg_bar.set_prefix("[2/2]");
        msg_bar.set_message("Broadcasting transaction...");

        let node_addr = provider.node_addr().await?;
        let params = AddParams {
            source: PublicKey(*node_addr.node_id.as_bytes()),
            key: key.into(),
            hash: Hash(*object_hash.as_bytes()),
            recovery_hash: Hash(*metadata_hash.as_bytes()),
            size,
            ttl: options.ttl,
            metadata: options.metadata,
            overwrite: options.overwrite,
            from,
        };

        let tx = signer
            .send_transaction(
                provider,
                self.address,
                options.token_amount.unwrap_or_default(),
                AddObject as u64,
                RawBytes::serialize(params)?,
                options.gas_params,
                options.broadcast_mode,
                decode_as,
            )
            .await?;

        msg_bar.println(format!(
            "{} Added object in {} (hash={}; size={})",
            SPARKLE,
            HumanDuration(started.elapsed()),
            object_hash,
            size
        ));
        msg_bar.finish_and_clear();
        Ok(tx)
    }

    /// Add an object into the bucket from a path.
    pub async fn add_from_path<C>(
        &self,
        provider: &impl Provider<C>,
        signer: &mut impl Signer,
        from: Address,
        key: &str,
        path: impl AsRef<Path>,
        options: AddOptions,
    ) -> anyhow::Result<TxResult<Object>>
    where
        C: Client + Send + Sync,
    {
        let path = path
            .as_ref()
            .canonicalize()
            .map_err(|e| anyhow!("failed to resolve path: {}", e))?;

        let mut file = tokio::fs::File::open(&path).await?;

        // Get total size using AsyncSeek
        let total_size = file.seek(std::io::SeekFrom::End(0)).await?;
        if total_size > MAX_OBJECT_LENGTH {
            return Err(anyhow!("file exceeds maximum allowed size of 5 GB"));
        }

        // Reset to start for upload
        file.seek(std::io::SeekFrom::Start(0)).await?;

        let content_type = mime_guess::from_path(&path)
            .first()
            .map(|mime| mime.to_string());
        let options = self.add_content_type_to_metadata(options, content_type);

        self.add_reader(provider, signer, from, key, file, total_size, options)
            .await
    }

    /// Delete an object.
    pub async fn delete<C>(
        &self,
        provider: &impl Provider<C>,
        signer: &mut impl Signer,
        from: Address,
        key: &str,
        options: DeleteOptions,
    ) -> anyhow::Result<TxResult<()>>
    where
        C: Client + Send + Sync,
    {
        let params = DeleteParams {
            key: key.into(),
            from,
        };
        let params = RawBytes::serialize(params)?;
        signer
            .send_transaction(
                provider,
                self.address,
                Default::default(),
                DeleteObject as u64,
                params,
                options.gas_params,
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

        let pro_bar = bars.add(new_progress_bar(object.size));
        let response = provider
            .download(self.address, key, options.range, options.height.into())
            .await?;
        let mut stream = response.bytes_stream();
        let mut progress = 0;
        while let Some(item) = stream.next().await {
            match item {
                Ok(chunk) => {
                    writer.write_all(&chunk).await?;
                    progress = min(progress + chunk.len(), object.size as usize);
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
        from: Address,
        key: &str,
        metadata: HashMap<String, Option<String>>,
        options: UpdateObjectMetadataOptions,
    ) -> anyhow::Result<TxResult<()>>
    where
        C: Client + Send + Sync,
    {
        validate_metadata_optional(&metadata)?;

        let params = UpdateObjectMetadataParams {
            key: key.into(),
            metadata,
            from,
        };
        let params = RawBytes::serialize(params)?;
        signer
            .send_transaction(
                provider,
                self.address,
                Default::default(),
                UpdateObjectMetadata as u64,
                params,
                options.gas_params,
                options.broadcast_mode,
                |_: &DeliverTx| -> anyhow::Result<()> { Ok(()) },
            )
            .await
    }

    fn add_content_type_to_metadata(
        &self,
        options: AddOptions,
        content_type: Option<String>,
    ) -> AddOptions {
        let mut metadata = options.metadata;
        if metadata.contains_key("content-type") {
            return AddOptions {
                metadata,
                ..options
            };
        }

        metadata.insert(
            "content-type".into(),
            content_type.unwrap_or("application/octet-stream".into()),
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

pub fn validate_metadata(metadata: &HashMap<String, String>) -> anyhow::Result<()> {
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
