// Copyright 2024 Hoku Contributors
// SPDX-License-Identifier: Apache-2.0, MIT

use std::collections::HashMap;

use anyhow::anyhow;
use async_trait::async_trait;
use bytes::Bytes;
use fendermint_actor_timehub::Method::{Count, Get, Peaks, Push, Root};
use fendermint_vm_actor_interface::adm::Kind;
use serde::{Deserialize, Serialize};
use tendermint::abci::response::DeliverTx;

use hoku_provider::{
    fvm_ipld_encoding::{self, BytesSer, RawBytes},
    fvm_shared::address::Address,
    message::{local_message, GasParams},
    query::{FvmQueryHeight, QueryProvider},
    response::{decode_bytes, Cid},
    tx::{BroadcastMode, TxReceipt},
    Client, Provider,
};
use hoku_signer::Signer;

use crate::machine::{deploy_machine, DeployTxReceipt, Machine};

const MAX_ACC_PAYLOAD_SIZE: usize = 1024 * 500;

/// Payload push options.
#[derive(Clone, Default, Debug)]
pub struct PushOptions {
    /// Broadcast mode for the transaction.
    pub broadcast_mode: BroadcastMode,
    /// Gas params for the transaction.
    pub gas_params: GasParams,
}

/// JSON serialization friendly version of [`fendermint_actor_timehub::PushReturn`].
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PushReturn {
    /// The new timehub root.
    pub root: Cid,
    /// The index of the newly pushed value.
    pub index: u64,
}

impl From<fendermint_actor_timehub::PushReturn> for PushReturn {
    fn from(v: fendermint_actor_timehub::PushReturn) -> Self {
        Self {
            root: v.root.into(),
            index: v.index,
        }
    }
}
/// JSON serialization friendly version of [`fendermint_actor_timehub::Leaf`].
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Leaf {
    /// Timestamp of the witness in seconds since the UNIX epoch
    pub timestamp: u64,
    /// Witnessed root CID
    pub witnessed: Cid,
}

impl From<fendermint_actor_timehub::Leaf> for Leaf {
    fn from(value: fendermint_actor_timehub::Leaf) -> Self {
        Self {
            timestamp: value.timestamp,
            witnessed: value.witnessed.into(),
        }
    }
}

/// A machine for event stream accumulation.
pub struct Timehub {
    address: Address,
}

#[async_trait]
impl Machine for Timehub {
    const KIND: Kind = Kind::Timehub;

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
            deploy_machine(provider, signer, owner, Kind::Timehub, metadata, gas_params).await?;
        Ok((Self::attach(address).await?, tx))
    }

    async fn attach(address: Address) -> anyhow::Result<Self> {
        Ok(Timehub { address })
    }

    fn address(&self) -> Address {
        self.address
    }
}

impl Timehub {
    /// Push a payload into the timehub.
    pub async fn push<C>(
        &self,
        provider: &impl Provider<C>,
        signer: &mut impl Signer,
        payload: Bytes,
        options: PushOptions,
    ) -> anyhow::Result<TxReceipt<PushReturn>>
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
        signer
            .send_transaction(
                provider,
                self.address,
                Default::default(),
                Push as u64,
                params,
                options.gas_params,
                decode_push_return,
            )
            .await
    }

    /// Get leaf stored at a given index and height.
    /// Returns None if there is no leaf at the given index.
    pub async fn leaf(
        &self,
        provider: &impl QueryProvider,
        index: u64,
        height: FvmQueryHeight,
    ) -> anyhow::Result<Option<Leaf>> {
        let params = RawBytes::serialize(index)?;
        let message = local_message(self.address, Get as u64, params);
        let response = provider.call(message, height, decode_leaf).await?;
        Ok(response.value)
    }

    /// Get total leaf count at a given height.
    pub async fn count(
        &self,
        provider: &impl QueryProvider,
        height: FvmQueryHeight,
    ) -> anyhow::Result<u64> {
        let message = local_message(self.address, Count as u64, Default::default());
        let response = provider.call(message, height, decode_count).await?;
        Ok(response.value)
    }

    /// Get all peaks at a given height.
    pub async fn peaks(
        &self,
        provider: &impl QueryProvider,
        height: FvmQueryHeight,
    ) -> anyhow::Result<Vec<Cid>> {
        let message = local_message(self.address, Peaks as u64, Default::default());
        let response = provider.call(message, height, decode_peaks).await?;
        Ok(response.value)
    }

    /// Get the root at a given height.
    pub async fn root(
        &self,
        provider: &impl QueryProvider,
        height: FvmQueryHeight,
    ) -> anyhow::Result<Cid> {
        let message = local_message(self.address, Root as u64, Default::default());
        let response = provider.call(message, height, decode_root).await?;
        Ok(response.value)
    }
}

fn decode_push_return(deliver_tx: &DeliverTx) -> anyhow::Result<PushReturn> {
    let data = decode_bytes(deliver_tx)?;
    fvm_ipld_encoding::from_slice::<fendermint_actor_timehub::PushReturn>(&data)
        .map(|r| r.into())
        .map_err(|e| anyhow!("error parsing as PushReturn: {e}"))
}

fn decode_leaf(deliver_tx: &DeliverTx) -> anyhow::Result<Option<Leaf>> {
    let data = decode_bytes(deliver_tx)?;
    Ok(
        fvm_ipld_encoding::from_slice::<Option<fendermint_actor_timehub::Leaf>>(&data)
            .map_err(|e| anyhow!("error parsing leaf: {e}"))?
            .map(|r| r.into()),
    )
}

fn decode_count(deliver_tx: &DeliverTx) -> anyhow::Result<u64> {
    let data = decode_bytes(deliver_tx)?;
    fvm_ipld_encoding::from_slice(&data).map_err(|e| anyhow!("error parsing as u64: {e}"))
}

fn decode_peaks(deliver_tx: &DeliverTx) -> anyhow::Result<Vec<Cid>> {
    let data = decode_bytes(deliver_tx)?;
    let items = fvm_ipld_encoding::from_slice::<Vec<cid::Cid>>(&data)
        .map(|v| v.iter().map(|c| (*c).into()).collect())
        .map_err(|e| anyhow!("error parsing as Vec<Cid>: {e}"))?;
    Ok(items)
}

fn decode_root(deliver_tx: &DeliverTx) -> anyhow::Result<Cid> {
    let data = decode_bytes(deliver_tx)?;
    let cid = fvm_ipld_encoding::from_slice::<cid::Cid>(&data)
        .map_err(|e| anyhow!("error parsing as Cid: {e}"))?;
    Ok(cid.into())
}
