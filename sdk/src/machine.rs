// Copyright 2024 ADM Contributors
// SPDX-License-Identifier: Apache-2.0, MIT

use anyhow::anyhow;
use async_trait::async_trait;
use fendermint_actor_machine::{WriteAccess, GET_METADATA_METHOD};
use fendermint_vm_actor_interface::adm::{
    CreateExternalParams, CreateExternalReturn, Kind, Method::CreateExternal, ADM_ACTOR_ADDR,
};
use fendermint_vm_message::query::FvmQueryHeight;
use fvm_ipld_encoding::RawBytes;
use fvm_shared::address::Address;
use serde::Serialize;
use tendermint::{abci::response::DeliverTx, block::Height, Hash};
use tendermint_rpc::Client;

use adm_provider::{message::local_message, response::decode_bytes, BroadcastMode, Provider};
use adm_signer::Signer;

use crate::{decode_metadata, TxArgs};

pub mod accumulator;
pub mod objectstore;

#[derive(Copy, Clone, Debug, Serialize)]
pub struct DeployTx {
    pub hash: Hash,
    pub height: Height,
    pub gas_used: i64,
}

#[async_trait]
pub trait Machine<C>: Send + Sync + Sized
where
    C: Client + Send + Sync,
{
    async fn new(
        provider: &impl Provider<C>,
        signer: &mut impl Signer,
        write_access: WriteAccess,
        args: TxArgs,
    ) -> anyhow::Result<(Self, DeployTx)>;

    fn attach(address: Address) -> Self;

    fn address(&self) -> Address;

    async fn owner(
        &self,
        provider: &impl Provider<C>,
        height: FvmQueryHeight,
    ) -> anyhow::Result<Address> {
        let message = local_message(self.address(), GET_METADATA_METHOD, Default::default());
        let response = provider.call(message, height, decode_metadata).await?;
        Ok(response.value.owner)
    }
}

async fn deploy_machine<C>(
    provider: &impl Provider<C>,
    signer: &mut impl Signer,
    kind: Kind,
    write_access: WriteAccess,
    args: TxArgs,
) -> anyhow::Result<(Address, DeployTx)>
where
    C: Client + Send + Sync,
{
    let params = CreateExternalParams { kind, write_access };
    let params = RawBytes::serialize(params)?;
    let message = signer.transaction(
        ADM_ACTOR_ADDR,
        Default::default(),
        CreateExternal as u64,
        params,
        None,
        args.gas_params,
    )?;
    let tx = provider
        .perform(message, BroadcastMode::Commit, decode_create)
        .await?;

    // In commit broadcast mode, if the data or address do not exist, something fatal happened.
    let address = tx
        .data
        .expect("data exists")
        .robust_address
        .expect("address exists");

    Ok((
        address,
        DeployTx {
            hash: tx.hash,
            height: tx.height.expect("height exists"),
            gas_used: tx.gas_used,
        },
    ))
}

fn decode_create(deliver_tx: &DeliverTx) -> anyhow::Result<CreateExternalReturn> {
    let data = decode_bytes(deliver_tx)?;
    fvm_ipld_encoding::from_slice::<CreateExternalReturn>(&data)
        .map_err(|e| anyhow!("error parsing as CreateExternalReturn: {e}"))
}
