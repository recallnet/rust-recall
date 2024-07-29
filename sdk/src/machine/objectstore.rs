// Copyright 2024 ADM Contributors
// SPDX-License-Identifier: Apache-2.0, MIT

use std::path::{Path, PathBuf};
use std::{cmp::min, collections::HashMap};

use anyhow::anyhow;
use async_trait::async_trait;
use base64::{engine::general_purpose, Engine};
use fendermint_actor_machine::WriteAccess;
use fendermint_actor_objectstore::{
    AddParams, DeleteParams, GetParams,
    Method::{AddObject, DeleteObject, GetObject, ListObjects},
    Object, ObjectList,
};
use fendermint_vm_actor_interface::adm::Kind;
use fendermint_vm_message::{query::FvmQueryHeight, signed::Object as MessageObject};
use fvm_ipld_encoding::RawBytes;
use fvm_shared::address::Address;
use indicatif::HumanDuration;
use iroh::blobs::{provider::AddProgress, util::SetTagOption};
use iroh::client::blobs::WrapOption;
use tendermint::abci::response::DeliverTx;
use tendermint_rpc::Client;
use tokio::fs::File;
use tokio::{
    io::{AsyncRead, AsyncSeek, AsyncWrite, AsyncWriteExt},
    time::Instant,
};
use tokio_stream::StreamExt;

use adm_provider::{
    message::{local_message, object_upload_message, GasParams},
    object::ObjectProvider,
    query::QueryProvider,
    response::{decode_bytes, decode_cid, Cid},
    tx::{BroadcastMode, TxReceipt},
    Provider,
};
use adm_signer::Signer;

use crate::progress::{new_message_bar, new_multi_bar, SPARKLE};
use crate::{
    machine::{deploy_machine, DeployTxReceipt, Machine},
    progress::new_progress_bar,
};

/// Object add options.
#[derive(Clone, Default, Debug)]
pub struct AddOptions {
    /// Overwrite the object if it already exists.
    pub overwrite: bool,
    /// Broadcast mode for the transaction.
    pub broadcast_mode: BroadcastMode,
    /// Gas params for the transaction.
    pub gas_params: GasParams,
    /// Whether to show progress-related output (useful for command-line interfaces).
    pub show_progress: bool,
    /// Metadata to add to the object.
    pub metadata: HashMap<String, String>,
}

/// Object delete options.
#[derive(Clone, Default, Debug)]
pub struct DeleteOptions {
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
    /// The offset to start listing objects from.
    pub offset: u64,
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
            offset: Default::default(),
            limit: Default::default(),
            height: Default::default(),
        }
    }
}

/// A machine for S3-like object storage.
pub struct ObjectStore {
    address: Address,
    iroh: iroh::node::MemNode,
}

#[async_trait]
impl Machine for ObjectStore {
    const KIND: Kind = Kind::ObjectStore;

    async fn new<C>(
        provider: &impl Provider<C>,
        signer: &mut impl Signer,
        write_access: WriteAccess,
        gas_params: GasParams,
    ) -> anyhow::Result<(Self, DeployTxReceipt)>
    where
        C: Client + Send + Sync,
    {
        let (address, tx) = deploy_machine(
            provider,
            signer,
            Kind::ObjectStore,
            write_access,
            gas_params,
        )
        .await?;
        let this = Self::attach(address).await?;
        Ok((this, tx))
    }

    async fn attach(address: Address) -> anyhow::Result<Self> {
        let node = iroh::node::Node::memory().spawn().await?;

        Ok(ObjectStore {
            address,
            iroh: node,
        })
    }

    fn address(&self) -> Address {
        self.address
    }
}

impl ObjectStore {
    /// Add an object into the object store with a reader.
    pub async fn add_reader<C, R>(
        &self,
        provider: &impl Provider<C>,
        signer: &mut impl Signer,
        key: &str,
        reader: R,
        options: AddOptions,
    ) -> anyhow::Result<TxReceipt<Cid>>
    where
        C: Client + Send + Sync,
        R: AsyncRead + AsyncSeek + Unpin + Send + 'static,
    {
        let started = Instant::now();
        let bars = new_multi_bar(!options.show_progress);
        let msg_bar = bars.add(new_message_bar());

        // TODO: This will blow up your memory, as we store the data in memory currently..

        let mut progress = self
            .iroh
            .blobs()
            .add_reader(reader, SetTagOption::Auto)
            .await?;

        // Iroh ingest
        msg_bar.set_prefix("[1/3]");
        msg_bar.set_message("Injesting data ...");

        let mut pro_bar = None;
        let mut object_size = 0;
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
                    object_size = size as usize;
                    pro_bar = Some(bars.add(new_progress_bar(size as _)));
                }
                AddProgress::Done { id: _, hash: _ } => {
                    pro_bar.take().unwrap().finish_and_clear();
                }
                AddProgress::AllDone { hash, .. } => {
                    break hash;
                }
                AddProgress::Progress { id: _, offset } => {
                    pro_bar.as_mut().unwrap().set_position(offset);
                }
                AddProgress::Abort(err) => {
                    return Err(err.into());
                }
            }
        };

        let object_cid = Cid(cid::Cid::new_v1(
            0x55,
            cid::multihash::Multihash::wrap(
                cid::multihash::Code::Blake3_256.into(),
                object_hash.as_ref(),
            )?,
        ));

        // Upload
        msg_bar.set_prefix("[2/3]");
        msg_bar.set_message(format!("Uploading {} to network...", object_cid));

        // TODO: progress bar
        self.upload(
            provider,
            signer,
            key,
            object_cid,
            object_size,
            options.metadata.clone(),
            options.overwrite,
        )
        .await?;

        // Broadcast transaction with Object's CID
        msg_bar.set_prefix("[3/3]");
        msg_bar.set_message("Broadcasting transaction...");
        let params = AddParams {
            key: key.into(),
            cid: object_cid.0,
            overwrite: options.overwrite,
            metadata: options.metadata,
            size: object_size,
        };
        let serialized_params = RawBytes::serialize(params.clone())?;
        let object = Some(MessageObject::new(
            params.key.clone(),
            object_cid.0,
            self.address,
        ));
        let message = signer
            .transaction(
                self.address,
                Default::default(),
                AddObject as u64,
                serialized_params,
                object,
                options.gas_params,
            )
            .await?;

        let tx = provider
            .perform(message, options.broadcast_mode, decode_cid)
            .await?;

        msg_bar.println(format!(
            "{} Added detached object in {} (cid={}; size={})",
            SPARKLE,
            HumanDuration(started.elapsed()),
            object_cid,
            object_size
        ));

        msg_bar.finish_and_clear();
        Ok(tx)
    }

    /// Add an object into the object store from a path.
    pub async fn add_from_path<C>(
        &self,
        provider: &impl Provider<C>,
        signer: &mut impl Signer,
        key: &str,
        path: impl AsRef<Path>,
        options: AddOptions,
    ) -> anyhow::Result<TxReceipt<Cid>>
    where
        C: Client + Send + Sync,
    {
        // TODO: Maybe duplicative of an Iroh check in `add_from_path` below
        // TODO: We could enable adding directories at some point
        // TODO: with a change to the on-chain object store actor
        let file = File::open(path).await?;
        let md = file.metadata().await?;
        if !md.is_file() {
            return Err(anyhow!("input must be a file"));
        }
        // TODO: Is this needed? not sure if having a ref will mess up the iroh method below
        drop(file);

        let started = Instant::now();
        let bars = new_multi_bar(!options.show_progress);
        let msg_bar = bars.add(new_message_bar());

        // TODO: This will blow up your memory, as we store the data in memory currently..

        let mut progress = self
            .iroh
            .blobs()
            .add_from_path(
                PathBuf::from(path.as_ref()),
                true,
                SetTagOption::Auto,
                WrapOption::NoWrap,
            )
            .await?;

        // Iroh ingest
        msg_bar.set_prefix("[1/3]");
        msg_bar.set_message("Injesting data ...");

        let mut pro_bar = None;
        let mut object_size = 0;
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
                    object_size = size as usize;
                    pro_bar = Some(bars.add(new_progress_bar(size as _)));
                }
                AddProgress::Done { id: _, hash: _ } => {
                    pro_bar.take().unwrap().finish_and_clear();
                }
                AddProgress::AllDone { hash, .. } => {
                    break hash;
                }
                AddProgress::Progress { id: _, offset } => {
                    pro_bar.as_mut().unwrap().set_position(offset);
                }
                AddProgress::Abort(err) => {
                    return Err(err.into());
                }
            }
        };

        let object_cid = Cid(cid::Cid::new_v1(
            0x55,
            cid::multihash::Multihash::wrap(
                cid::multihash::Code::Blake3_256.into(),
                object_hash.as_ref(),
            )?,
        ));

        // Upload
        msg_bar.set_prefix("[2/3]");
        msg_bar.set_message(format!("Uploading {} to network...", object_cid));

        // TODO: progress bar
        self.upload(
            provider,
            signer,
            key,
            object_cid,
            object_size,
            options.metadata.clone(),
            options.overwrite,
        )
        .await?;

        // Broadcast transaction with Object's CID
        msg_bar.set_prefix("[3/3]");
        msg_bar.set_message("Broadcasting transaction...");
        let params = AddParams {
            key: key.into(),
            cid: object_cid.0,
            overwrite: options.overwrite,
            metadata: options.metadata,
            size: object_size,
        };
        let serialized_params = RawBytes::serialize(params.clone())?;
        let object = Some(MessageObject::new(
            params.key.clone(),
            object_cid.0,
            self.address,
        ));
        let message = signer
            .transaction(
                self.address,
                Default::default(),
                AddObject as u64,
                serialized_params,
                object,
                options.gas_params,
            )
            .await?;

        let tx = provider
            .perform(message, options.broadcast_mode, decode_cid)
            .await?;

        msg_bar.println(format!(
            "{} Added detached object in {} (cid={}; size={})",
            SPARKLE,
            HumanDuration(started.elapsed()),
            object_cid,
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
        signer: &mut impl Signer,
        key: &str,
        cid: Cid,
        size: usize,
        metadata: HashMap<String, String>,
        overwrite: bool,
    ) -> anyhow::Result<()> {
        let from = signer.address();
        let params = AddParams {
            key: key.into(),
            cid: cid.0,
            overwrite,
            metadata,
            size,
        };
        let serialized_params = RawBytes::serialize(params)?;

        let message =
            object_upload_message(from, self.address, AddObject as u64, serialized_params);
        let singed_message = signer.sign_message(
            message,
            Some(MessageObject::new(key.into(), cid.0, self.address)),
        )?;
        let serialized_signed_message = fvm_ipld_encoding::to_vec(&singed_message)?;

        let chain_id = match signer.subnet_id() {
            Some(id) => id.chain_id(),
            None => {
                return Err(anyhow!("failed to get subnet ID from signer"));
            }
        };

        let node_addr = self.iroh.node_addr().await?;
        provider
            .upload(
                cid,
                node_addr,
                size,
                general_purpose::URL_SAFE.encode(&serialized_signed_message),
                chain_id.into(),
            )
            .await?;

        Ok(())
    }

    /// Delete an object.
    pub async fn delete<C>(
        &self,
        provider: &impl Provider<C>,
        signer: &mut impl Signer,
        key: &str,
        options: DeleteOptions,
    ) -> anyhow::Result<TxReceipt<Cid>>
    where
        C: Client + Send + Sync,
    {
        let params = DeleteParams { key: key.into() };
        let params = RawBytes::serialize(params)?;
        let message = signer
            .transaction(
                self.address,
                Default::default(),
                DeleteObject as u64,
                params,
                None,
                options.gas_params,
            )
            .await?;
        provider
            .perform(message, options.broadcast_mode, decode_cid)
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
        let params = GetParams { key: key.into() };
        let params = RawBytes::serialize(params)?;
        let message = local_message(self.address, GetObject as u64, params);
        let response = provider.call(message, options.height, decode_get).await?;

        let object = response
            .value
            .ok_or_else(|| anyhow!("object not found for key '{}'", key))?;

        let cid = cid::Cid::try_from(object.cid.0)?;
        if !object.resolved {
            return Err(anyhow!("object is not resolved"));
        }
        msg_bar.set_prefix("[2/2]");
        msg_bar.set_message(format!("Downloading {}... ", cid));

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
                    progress = min(progress + chunk.len(), object_size);
                    pro_bar.set_position(progress as u64);
                }
                Err(e) => {
                    return Err(anyhow!(e));
                }
            }
        }
        pro_bar.finish_and_clear();
        msg_bar.println(format!(
            "{} Downloaded detached object in {} (cid={})",
            SPARKLE,
            HumanDuration(started.elapsed()),
            cid
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
    ) -> anyhow::Result<ObjectList> {
        let params = fendermint_actor_objectstore::ListParams {
            prefix: options.prefix.into(),
            delimiter: options.delimiter.into(),
            offset: options.offset,
            limit: options.limit,
        };
        let params = RawBytes::serialize(params)?;
        let message = local_message(self.address, ListObjects as u64, params);
        let response = provider.call(message, options.height, decode_list).await?;
        Ok(response.value)
    }
}

fn decode_get(deliver_tx: &DeliverTx) -> anyhow::Result<Option<Object>> {
    let data = decode_bytes(deliver_tx)?;
    fvm_ipld_encoding::from_slice(&data)
        .map_err(|e| anyhow!("error parsing as Option<Object>: {e}"))
}

fn decode_list(deliver_tx: &DeliverTx) -> anyhow::Result<ObjectList> {
    let data = decode_bytes(deliver_tx)?;
    fvm_ipld_encoding::from_slice(&data).map_err(|e| anyhow!("error parsing as ObjectList: {e}"))
}
