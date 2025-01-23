// Copyright 2024 Hoku Contributors
// SPDX-License-Identifier: Apache-2.0, MIT

use std::collections::HashMap;

use anyhow::anyhow;
use async_trait::async_trait;
use fendermint_actor_sqlite::Method;
use fendermint_vm_actor_interface::adm::Kind;
use tendermint::abci::response::DeliverTx;

use hoku_provider::{
    fvm_ipld_encoding,
    fvm_shared::address::Address,
    message::{local_message, GasParams, RawBytes},
    query::{FvmQueryHeight, QueryProvider},
    response::decode_bytes,
    tx::{BroadcastMode, TxReceipt},
    Client, Provider,
};
use hoku_signer::Signer;

pub use fendermint_actor_sqlite::{ExecuteParams, ExecuteReturn, QueryParams, QueryReturn};

use crate::machine::{deploy_machine, DeployTxReceipt, Machine};

/// Payload push options.
#[derive(Clone, Default, Debug)]
pub struct ExecuteOptions {
    /// Broadcast mode for the transaction.
    pub broadcast_mode: BroadcastMode,
    /// Gas params for the transaction.
    pub gas_params: GasParams,
}

/// A machine for event stream accumulation.
pub struct Sqlite {
    address: Address,
}

#[async_trait]
impl Machine for Sqlite {
    const KIND: Kind = Kind::Sqlite;

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
            deploy_machine(provider, signer, owner, Self::KIND, metadata, gas_params).await?;
        Ok((Self::attach(address).await?, tx))
    }

    async fn attach(address: Address) -> anyhow::Result<Self> {
        Ok(Sqlite { address })
    }

    fn address(&self) -> Address {
        self.address
    }
}

impl Sqlite {
    /// Query the sqlite actor
    pub async fn query(
        &self,
        provider: &impl QueryProvider,
        stmt: String,
        height: FvmQueryHeight,
    ) -> anyhow::Result<QueryReturn> {
        if stmt.is_empty() {
            return Err(anyhow!("query must not be an empty string"));
        }
        let params = RawBytes::serialize(QueryParams { stmt })?;

        let message = local_message(self.address, Method::Query as u64, params);
        let response = provider.call(message, height, decode_query_result).await?;
        Ok(response.value)
    }

    pub async fn execute<C>(
        &self,
        provider: &impl Provider<C>,
        signer: &mut impl Signer,
        stmts: Vec<String>,
        options: ExecuteOptions,
    ) -> anyhow::Result<TxReceipt<ExecuteReturn>>
    where
        C: Client + Send + Sync,
    {
        let params = RawBytes::serialize(ExecuteParams { stmts })?;
        let msg = local_message(self.address, Method::Execute as u64, params.clone());
        let estimated_gas = provider.estimate_gas_limit(msg, Default::default()).await?;
        let gas_params = GasParams {
            gas_limit: estimated_gas,
            ..options.gas_params
        };
        signer
            .send_transaction(
                provider,
                self.address,
                Default::default(),
                Method::Execute as u64,
                params,
                gas_params,
                options.broadcast_mode,
                decode_execute_result,
            )
            .await
    }
}

fn decode_query_result(deliver_tx: &DeliverTx) -> anyhow::Result<QueryReturn> {
    let data = decode_bytes(deliver_tx)?;
    let res = fvm_ipld_encoding::from_slice::<QueryReturn>(&data)
        .map_err(|e| anyhow!("error parsing query return: {e}"))?;
    Ok(res)
}

fn decode_execute_result(deliver_tx: &DeliverTx) -> anyhow::Result<ExecuteReturn> {
    let data = decode_bytes(deliver_tx)?;
    let res = fvm_ipld_encoding::from_slice::<ExecuteReturn>(&data)
        .map_err(|e| anyhow!("error parsing execute return: {e}"))?;
    Ok(res)
}
