// Copyright 2024 ADM Contributors
// SPDX-License-Identifier: Apache-2.0, MIT

use std::marker::PhantomData;

use anyhow::anyhow;
use async_tempfile::TempFile;
use async_trait::async_trait;
use base64::{engine::general_purpose, Engine};
use bytes::Bytes;
use cid::multihash::{Code, MultihashDigest};
use cid::Cid;
use fendermint_actor_machine::WriteAccess;
use fendermint_actor_objectstore::{
    DeleteParams, GetParams, ListParams,
    Method::{DeleteObject, GetObject, ListObjects, PutObject},
    Object, ObjectKind, ObjectList, PutParams,
};
use fendermint_vm_actor_interface::adm::Kind;
use fendermint_vm_message::{query::FvmQueryHeight, signed::Object as MessageObject};
use fvm_ipld_encoding::RawBytes;
use fvm_shared::address::Address;
use ipfs_unixfs::file::adder::{Chunker, FileAdder};
use reqwest;
use tendermint::abci::response::DeliverTx;
use tendermint_rpc::Client;
use tokio::io::AsyncReadExt;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::StreamExt;

use adm_provider::{
    message::{local_message, object_upload_message},
    response::{decode_bytes, decode_cid},
    upload::ObjectUploader,
    BroadcastMode, Provider, Tx,
};
use adm_signer::Signer;

use crate::machine::{deploy_machine, DeployTx, Machine};
use crate::TxArgs;

pub struct ObjectStore<C> {
    address: Address,
    _marker: PhantomData<C>,
}

#[async_trait]
impl<C> Machine<C> for ObjectStore<C>
where
    C: Client + Send + Sync,
{
    async fn new(
        provider: &impl Provider<C>,
        signer: &mut impl Signer,
        write_access: WriteAccess,
        args: TxArgs,
    ) -> anyhow::Result<(Self, DeployTx)> {
        let (address, tx) =
            deploy_machine(provider, signer, Kind::ObjectStore, write_access, args).await?;
        Ok((Self::attach(address), tx))
    }

    fn attach(address: Address) -> Self {
        ObjectStore {
            address,
            _marker: PhantomData,
        }
    }

    fn address(&self) -> Address {
        self.address
    }
}

impl<C> ObjectStore<C>
where
    C: Client + Send + Sync,
{
    pub async fn object_upload(
        &self,
        signer: &mut impl Signer,
        object_client: impl ObjectUploader,
        key: String,
        cid: Cid,
        rx: mpsc::Receiver<Vec<u8>>,
        size: usize,
        params: PutParams,
    ) -> anyhow::Result<Cid> {
        let from = signer.address();
        let serialized_params = RawBytes::serialize(params)?;
        let message =
            object_upload_message(from, self.address, PutObject as u64, serialized_params);
        let singed_message = signer.sign_message(
            message,
            Some(MessageObject::new(
                key.as_bytes().to_vec(),
                cid,
                self.address,
            )),
        )?;
        let serialized_signed_message = fvm_ipld_encoding::to_vec(&singed_message)?;

        let object_stream = ReceiverStream::new(rx)
            .map(|bytes_vec| Ok::<Bytes, reqwest::Error>(Bytes::from(bytes_vec)));
        let body = reqwest::Body::wrap_stream(object_stream);
        let response = object_client
            .upload(
                body,
                size,
                general_purpose::URL_SAFE.encode(&serialized_signed_message),
            )
            .await?;
        Ok(response.cid)
    }

    pub async fn put(
        &self,
        provider: &impl Provider<C>,
        signer: &mut impl Signer,
        params: PutParams,
        broadcast_mode: BroadcastMode,
        args: TxArgs,
    ) -> anyhow::Result<Tx<Cid>> {
        let object = match &params.kind {
            ObjectKind::Internal(_) => None,
            ObjectKind::External(cid) => {
                Some(MessageObject::new(params.key.clone(), *cid, self.address))
            }
        };
        let params = RawBytes::serialize(params)?;
        let message = signer.transaction(
            self.address,
            Default::default(),
            PutObject as u64,
            params,
            object,
            args.gas_params,
        )?;
        provider.perform(message, broadcast_mode, decode_cid).await
    }

    pub async fn delete(
        &self,
        provider: &impl Provider<C>,
        signer: &mut impl Signer,
        params: DeleteParams,
        broadcast_mode: BroadcastMode,
        args: TxArgs,
    ) -> anyhow::Result<Tx<Cid>> {
        let params = RawBytes::serialize(params)?;
        let message = signer.transaction(
            self.address,
            Default::default(),
            DeleteObject as u64,
            params,
            None,
            args.gas_params,
        )?;
        provider.perform(message, broadcast_mode, decode_cid).await
    }

    pub async fn get(
        &self,
        provider: &impl Provider<C>,
        params: GetParams,
        height: FvmQueryHeight,
    ) -> anyhow::Result<Option<Object>> {
        let params = RawBytes::serialize(params)?;
        let message = local_message(self.address, GetObject as u64, params);
        let response = provider.call(message, height, decode_get).await?;
        Ok(response.value)
    }

    pub async fn list(
        &self,
        provider: &impl Provider<C>,
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

pub async fn generate_cid(tmp: &mut TempFile) -> anyhow::Result<Cid> {
    let chunk_size = 1024 * 1024;
    let mut adder = FileAdder::builder()
        .with_chunker(Chunker::Size(chunk_size))
        .build();
    let mut tmp_buffer = vec![0; chunk_size];
    loop {
        match tmp.read(&mut tmp_buffer).await {
            Ok(0) => {
                break;
            }
            Ok(n) => {
                let _ = adder.push(&tmp_buffer[..n]);
            }
            Err(e) => {
                return Err(e.into());
            }
        }
    }
    let unixfs_iterator = adder.finish();
    let last_chunk = unixfs_iterator.last().unwrap();
    let hash = Code::Sha2_256.digest(&last_chunk.1);
    let cid = Cid::new_v0(hash)?;
    Ok(cid)
}
