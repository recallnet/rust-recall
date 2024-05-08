// Copyright 2024 ADM Contributors
// SPDX-License-Identifier: Apache-2.0, MIT

use anyhow::anyhow;
use async_trait::async_trait;
use bytes::Bytes;
use fendermint_actor_accumulator::Method::{Push, Root};
use fendermint_actor_machine::WriteAccess;
use fendermint_vm_actor_interface::adm::Kind;
use fendermint_vm_message::query::FvmQueryHeight;
use fvm_ipld_encoding::{BytesSer, RawBytes};
use fvm_shared::address::Address;
use serde::{Deserialize, Serialize};
use tendermint::abci::response::DeliverTx;
use tendermint_rpc::Client;

use adm_provider::{
    message::local_message, response::decode_bytes, response::decode_cid, response::Cid,
    BroadcastMode, Provider, QueryProvider, Tx,
};
use adm_signer::Signer;

use crate::machine::{deploy_machine, DeployTx, Machine};
use crate::TxArgs;

const MAX_ACC_PAYLOAD_SIZE: usize = 1024 * 500;

/// Pretty version of [`fendermint_actor_accumulator::PushReturn`].
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PushReturn {
    pub root: Cid,
    pub index: u64,
}

impl From<fendermint_actor_accumulator::PushReturn> for PushReturn {
    fn from(v: fendermint_actor_accumulator::PushReturn) -> Self {
        Self {
            root: v.root.into(),
            index: v.index,
        }
    }
}

pub struct Accumulator {
    address: Address,
}

#[async_trait]
impl Machine for Accumulator {
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
            deploy_machine(provider, signer, Kind::Accumulator, write_access, args).await?;
        Ok((Self::attach(address), tx))
    }

    fn attach(address: Address) -> Self {
        Accumulator { address }
    }

    fn address(&self) -> Address {
        self.address
    }
}

impl Accumulator {
    pub async fn push<C>(
        &self,
        provider: &impl Provider<C>,
        signer: &mut impl Signer,
        payload: Bytes,
        broadcast_mode: BroadcastMode,
        args: TxArgs,
    ) -> anyhow::Result<Tx<PushReturn>>
    where
        C: Client + Send + Sync,
    {
        if payload.len() > MAX_ACC_PAYLOAD_SIZE {
            return Err(anyhow!(
                "max payload size is {} bytes",
                MAX_ACC_PAYLOAD_SIZE
            ));
        }

        let params = RawBytes::serialize(BytesSer(&payload))?;
        let message = signer.transaction(
            self.address,
            Default::default(),
            Push as u64,
            params,
            None,
            args.gas_params,
        ).await?;
        provider
            .perform(message, broadcast_mode, decode_acc_push_return)
            .await
    }

    pub async fn root(
        &self,
        provider: &impl QueryProvider,
        height: FvmQueryHeight,
    ) -> anyhow::Result<Cid> {
        let message = local_message(self.address, Root as u64, Default::default());
        let response = provider.call(message, height, decode_cid).await?;
        Ok(response.value)
    }
}

fn decode_acc_push_return(deliver_tx: &DeliverTx) -> anyhow::Result<PushReturn> {
    let data = decode_bytes(deliver_tx)?;
    fvm_ipld_encoding::from_slice::<fendermint_actor_accumulator::PushReturn>(&data)
        .map(|r| r.into())
        .map_err(|e| anyhow!("error parsing as PushReturn: {e}"))
}
