// Copyright 2024 ADM Contributors
// SPDX-License-Identifier: Apache-2.0, MIT

use anyhow::anyhow;
use fendermint_actor_blobs::Method::{FundAccount, GetStatus};
use fendermint_actor_blobs::{Account, FundParams, Status};
use fendermint_vm_actor_interface::blobs::BLOBS_ACTOR_ADDR;
use fendermint_vm_message::query::{ActorState, FvmQueryHeight};
use fvm_ipld_encoding::RawBytes;
use fvm_shared::address::Address;
use fvm_shared::econ::TokenAmount;
use tendermint::abci::response::DeliverTx;
use tendermint_rpc::Client;

use adm_provider::message::{local_message, GasParams};
use adm_provider::query::QueryProvider;
use adm_provider::response::decode_bytes;
use adm_provider::tx::{BroadcastMode, TxReceipt};
use adm_provider::Provider;
use adm_signer::Signer;

#[derive(Clone, Default, Debug)]
pub struct FundOptions {
    /// Broadcast mode for the transaction.
    pub broadcast_mode: BroadcastMode,
    /// Gas params for the transaction.
    pub gas_params: GasParams,
}

/// A static wrapper around ADM blob methods.
pub struct Blobs {}

impl Blobs {
    pub async fn state(
        provider: &impl QueryProvider,
        height: FvmQueryHeight,
    ) -> anyhow::Result<ActorState> {
        let response = provider.actor_state(&BLOBS_ACTOR_ADDR, height).await?;

        match response.value {
            Some((_, state)) => Ok(state),
            None => Err(anyhow!("failed to get blobs actor",)),
        }
    }

    pub async fn query(
        provider: &impl QueryProvider,
        height: FvmQueryHeight,
    ) -> anyhow::Result<Status> {
        let message = local_message(BLOBS_ACTOR_ADDR, GetStatus as u64, Default::default());
        let response = provider.call(message, height, decode_status).await?;
        Ok(response.value)
    }

    pub async fn fund<C>(
        provider: &impl Provider<C>,
        signer: &mut impl Signer,
        address: Address,
        amount: TokenAmount,
        options: FundOptions,
    ) -> anyhow::Result<TxReceipt<Account>>
    where
        C: Client + Send + Sync,
    {
        let params = FundParams(address);
        let params = RawBytes::serialize(params)?;
        let message = signer
            .transaction(
                BLOBS_ACTOR_ADDR,
                amount,
                FundAccount as u64,
                params,
                options.gas_params,
            )
            .await?;
        provider
            .perform(message, options.broadcast_mode, decode_fund)
            .await
    }
}

fn decode_status(deliver_tx: &DeliverTx) -> anyhow::Result<Status> {
    let data = decode_bytes(deliver_tx)?;
    fvm_ipld_encoding::from_slice(&data).map_err(|e| anyhow!("error parsing as Status: {e}"))
}

fn decode_fund(deliver_tx: &DeliverTx) -> anyhow::Result<Account> {
    let data = decode_bytes(deliver_tx)?;
    fvm_ipld_encoding::from_slice(&data).map_err(|e| anyhow!("error parsing as Account: {e}"))
}
