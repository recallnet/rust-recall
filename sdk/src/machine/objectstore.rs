// Copyright 2024 ADM Contributors
// SPDX-License-Identifier: Apache-2.0, MIT

use std::marker::PhantomData;

use anyhow::anyhow;
use async_trait::async_trait;
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
use tendermint::abci::response::DeliverTx;
use tendermint_rpc::Client;

use adm_provider::{
    message::local_message,
    response::{decode_bytes, decode_cid},
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
