// Copyright 2024 Hoku Contributors
// SPDX-License-Identifier: Apache-2.0, MIT

use std::collections::HashMap;

use anyhow::anyhow;
use async_trait::async_trait;
use fendermint_actor_machine::{Metadata, GET_METADATA_METHOD};
use fendermint_vm_actor_interface::adm::{
    self, CreateExternalParams, CreateExternalReturn, Kind, ListMetadataParams,
    Method::CreateExternal, Method::ListMetadata, ADM_ACTOR_ADDR,
};
use fendermint_vm_message::query::FvmQueryHeight;
use fvm_ipld_encoding::RawBytes;
use fvm_shared::address::Address;
use serde::Serialize;
use tendermint::{abci::response::DeliverTx, block::Height, Hash};
use tendermint_rpc::Client;

use hoku_provider::{
    message::{local_message, GasParams},
    query::QueryProvider,
    response::decode_bytes,
    tx::BroadcastMode,
    Provider,
};
use hoku_signer::Signer;

pub mod bucket;
pub mod timehub;

/// Deployed machine transaction receipt details.
#[derive(Copy, Clone, Debug, Serialize)]
pub struct DeployTxReceipt {
    pub hash: Hash,
    pub height: Height,
    pub gas_used: i64,
}

/// Trait implemented by different machine kinds.
/// This is modeled after Ethers contract deployment UX.
#[async_trait]
pub trait Machine: Send + Sync + Sized {
    const KIND: Kind;

    /// Create a new machine instance using the given [`Provider`] and [`Signer`].
    async fn new<C>(
        provider: &impl Provider<C>,
        signer: &mut impl Signer,
        owner: Option<Address>,
        metadata: HashMap<String, String>,
        gas_params: GasParams,
    ) -> anyhow::Result<(Self, DeployTxReceipt)>
    where
        C: Client + Send + Sync;

    /// List machines owned by the given [`Signer`].
    async fn list(
        provider: &impl QueryProvider,
        signer: &impl Signer,
        height: FvmQueryHeight,
    ) -> anyhow::Result<Vec<adm::Metadata>> {
        let input = ListMetadataParams {
            owner: signer.address(),
        };
        let params = RawBytes::serialize(input)?;
        let message = local_message(ADM_ACTOR_ADDR, ListMetadata as u64, params);
        let response = provider.call(message, height, decode_list).await?;

        // Filtering "kind" on the client is a bit silly.
        // Maybe we can add a filter on "kind" in the adm actor.
        // TODO: Implement PartialEq on Kind to avoid the string comparison.
        let list: Vec<adm::Metadata> = response
            .value
            .into_iter()
            .filter(|m| m.kind.to_string() == Self::KIND.to_string())
            .collect::<Vec<adm::Metadata>>();

        Ok(list)
    }

    /// Create a machine instance from an existing machine [`Address`].
    async fn attach(address: Address) -> anyhow::Result<Self>;

    /// Returns the machine [`Address`].
    fn address(&self) -> Address;
}

/// Get machine info (the owner and machine kind).
pub async fn info(
    provider: &impl QueryProvider,
    address: Address,
    height: FvmQueryHeight,
) -> anyhow::Result<Metadata> {
    let message = local_message(address, GET_METADATA_METHOD, Default::default());
    let response = provider.call(message, height, decode_info).await?;
    Ok(response.value)
}

/// Deploys a machine.
async fn deploy_machine<C>(
    provider: &impl Provider<C>,
    signer: &mut impl Signer,
    owner: Option<Address>,
    kind: Kind,
    metadata: HashMap<String, String>,
    gas_params: GasParams,
) -> anyhow::Result<(Address, DeployTxReceipt)>
where
    C: Client + Send + Sync,
{
    let params = CreateExternalParams {
        owner: owner.unwrap_or(signer.address()),
        kind,
        metadata,
    };

    let params = RawBytes::serialize(params)?;
    let message = signer
        .transaction(
            ADM_ACTOR_ADDR,
            Default::default(),
            CreateExternal as u64,
            params,
            gas_params,
        )
        .await?;
    let tx = provider
        .perform(message, BroadcastMode::Commit, decode_create)
        .await?;

    // In commit broadcast mode, if the data or address does not exist, something fatal happened.
    let address = tx
        .data
        .expect("data exists")
        .robust_address
        .expect("address exists");

    Ok((
        address,
        DeployTxReceipt {
            hash: tx.hash,
            height: tx.height.expect("height exists"),
            gas_used: tx.gas_used,
        },
    ))
}

fn decode_create(deliver_tx: &DeliverTx) -> anyhow::Result<CreateExternalReturn> {
    let data = decode_bytes(deliver_tx)?;
    fvm_ipld_encoding::from_slice(&data)
        .map_err(|e| anyhow!("error parsing as CreateExternalReturn: {e}"))
}

fn decode_list(deliver_tx: &DeliverTx) -> anyhow::Result<Vec<adm::Metadata>> {
    let data = decode_bytes(deliver_tx)?;
    fvm_ipld_encoding::from_slice(&data)
        .map_err(|e| anyhow!("error parsing as Vec<adm::Metadata>: {e}"))
}

fn decode_info(deliver_tx: &DeliverTx) -> anyhow::Result<Metadata> {
    let data = decode_bytes(deliver_tx)?;
    fvm_ipld_encoding::from_slice(&data).map_err(|e| anyhow!("error parsing as Metadata: {e}"))
}
