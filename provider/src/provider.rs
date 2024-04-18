// Copyright 2024 ADM Contributors
// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

use std::str::FromStr;

use anyhow::{anyhow, Context};
use async_trait::async_trait;
use cid::Cid;
use fendermint_vm_message::{
    chain::ChainMessage,
    query::{ActorState, BuiltinActors, FvmQuery, FvmQueryHeight, GasEstimate, StateParams},
};
use fvm_shared::{address::Address, error::ExitCode, message::Message, ActorID};
use num_traits::Zero;
use prost::Message as ProstMessage;
use serde::Serialize;
use tendermint::{abci::response::DeliverTx, block::Height, Hash};
use tendermint_proto::abci::ResponseDeliverTx;
use tendermint_rpc::{endpoint::abci_query::AbciQuery, Client};

use crate::response::encode_data;

/// Provider capable of submitting queries and transactions.
pub trait Provider<C>: TendermintClient<C> + QueryProvider + TxProvider
where
    C: Client + Send + Sync,
{
}

/// Get to the underlying Tendermint client if necessary, for example to query the state of transactions.
pub trait TendermintClient<C>
where
    C: Client + Send + Sync,
{
    /// The underlying Tendermint client.
    fn underlying(&self) -> &C;
}

/// The parsed value from a query, along with the height at which the query was performed.
#[derive(Debug, Clone, Serialize)]
pub struct QueryResponse<T> {
    pub height: Height,
    pub value: T,
}

/// Provider for submitting queries.
#[async_trait]
pub trait QueryProvider: Send + Sync {
    /// Run a message in a read-only fashion.
    async fn call<F, T>(
        &self,
        message: Message,
        height: FvmQueryHeight,
        f: F,
    ) -> anyhow::Result<QueryResponse<T>>
    where
        F: FnOnce(&DeliverTx) -> anyhow::Result<T> + Sync + Send,
        T: Sync + Send,
    {
        let res = self
            .query(FvmQuery::Call(Box::new(message)), height)
            .await?;
        let height = res.height;
        let tx = extract(res, parse_deliver_tx)?;
        let value = f(&tx)?;
        Ok(QueryResponse { height, value })
    }

    /// Estimate the gas limit of a message.
    async fn estimate_gas(
        &self,
        mut message: Message,
        height: FvmQueryHeight,
    ) -> anyhow::Result<QueryResponse<GasEstimate>> {
        // Using 0 sequence so estimation doesn't get tripped over by nonce mismatch.
        message.sequence = 0;

        let res = self
            .query(FvmQuery::EstimateGas(Box::new(message)), height)
            .await?;
        let height = res.height;
        let value = extract(res, |res| {
            fvm_ipld_encoding::from_slice(&res.value)
                .context("failed to decode GasEstimate from query")
        })?;
        Ok(QueryResponse { height, value })
    }

    /// Query the state of an actor.
    async fn actor_state(
        &self,
        address: &Address,
        height: FvmQueryHeight,
    ) -> anyhow::Result<QueryResponse<Option<(ActorID, ActorState)>>> {
        let res = self.query(FvmQuery::ActorState(*address), height).await?;
        let height = res.height;
        let value = extract_actor_state(res)?;
        Ok(QueryResponse { height, value })
    }

    /// Query the contents of a CID from the IPLD store.
    async fn ipld(&self, cid: &Cid, height: FvmQueryHeight) -> anyhow::Result<Option<Vec<u8>>> {
        let res = self.query(FvmQuery::Ipld(*cid), height).await?;
        extract_opt(res, |res| Ok(res.value))
    }

    /// Slowly changing state parameters.
    async fn state_params(
        &self,
        height: FvmQueryHeight,
    ) -> anyhow::Result<QueryResponse<StateParams>> {
        let res = self.query(FvmQuery::StateParams, height).await?;
        let height = res.height;
        let value = extract(res, |res| {
            fvm_ipld_encoding::from_slice(&res.value)
                .context("failed to decode StateParams from query")
        })?;
        Ok(QueryResponse { height, value })
    }

    /// Queries the built-in actors known by the System actor.
    async fn builtin_actors(
        &self,
        height: FvmQueryHeight,
    ) -> anyhow::Result<QueryResponse<BuiltinActors>> {
        let res = self.query(FvmQuery::BuiltinActors, height).await?;
        let height = res.height;
        let value = {
            let registry: Vec<(String, Cid)> = extract(res, |res| {
                fvm_ipld_encoding::from_slice(&res.value)
                    .context("failed to decode BuiltinActors from query")
            })?;
            BuiltinActors { registry }
        };
        Ok(QueryResponse { height, value })
    }

    /// Run an ABCI query.
    async fn query(&self, query: FvmQuery, height: FvmQueryHeight) -> anyhow::Result<AbciQuery>;
}

#[derive(Debug, Default, Copy, Clone)]
pub enum BroadcastMode {
    /// Return immediately after the transaction is broadcasted without waiting for check results.
    Async,
    /// Wait for the check results before returning from broadcast.
    Sync,
    /// Wait for the delivery results before returning from broadcast.
    #[default]
    Commit,
}

impl FromStr for BroadcastMode {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "async" => Self::Async,
            "sync" => Self::Sync,
            "commit" => Self::Commit,
            _ => return Err(anyhow!("invalid broadcast mode")),
        })
    }
}

#[derive(Debug, Copy, Clone, Serialize)]
pub enum TxStatus {
    Pending,
    Committed,
}

#[derive(Debug, Copy, Clone, Serialize)]
pub struct Tx<T> {
    pub status: TxStatus,
    pub hash: Hash,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<Height>,
    #[serde(skip_serializing_if = "i64::is_zero")]
    pub gas_used: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
}

impl<D> Tx<D> {
    pub fn pending(hash: Hash) -> Self {
        Tx {
            status: TxStatus::Pending,
            hash,
            height: None,
            gas_used: 0,
            data: None,
        }
    }

    pub fn committed(hash: Hash, height: Height, gas_used: i64, data: Option<D>) -> Self {
        Tx {
            status: TxStatus::Committed,
            hash,
            height: Some(height),
            gas_used,
            data,
        }
    }
}

/// Provider for submitting transactions.
#[async_trait]
pub trait TxProvider: Send + Sync {
    /// Perform the sending of a chain message.
    async fn perform<F, T>(
        &self,
        message: ChainMessage,
        broadcast_mode: BroadcastMode,
        f: F,
    ) -> anyhow::Result<Tx<T>>
    where
        F: FnOnce(&DeliverTx) -> anyhow::Result<T> + Sync + Send,
        T: Sync + Send;
}

/// Extract some value from the query result, unless it's not found or other error.
fn extract_opt<T, F>(res: AbciQuery, f: F) -> anyhow::Result<Option<T>>
where
    F: FnOnce(AbciQuery) -> anyhow::Result<T>,
{
    if is_not_found(&res) {
        Ok(None)
    } else {
        extract(res, f).map(Some)
    }
}

/// Extract some value from the query result, unless there was an error.
fn extract<T, F>(res: AbciQuery, f: F) -> anyhow::Result<T>
where
    F: FnOnce(AbciQuery) -> anyhow::Result<T>,
{
    if res.code.is_err() {
        Err(anyhow!(
            "query returned non-zero exit code: {}; info: {}; log: {}",
            res.code.value(),
            res.info,
            res.log
        ))
    } else {
        f(res)
    }
}

fn extract_actor_state(res: AbciQuery) -> anyhow::Result<Option<(ActorID, ActorState)>> {
    extract_opt(res, |res| {
        let state: ActorState =
            fvm_ipld_encoding::from_slice(&res.value).context("failed to decode state")?;

        let id: ActorID = fvm_ipld_encoding::from_slice(&res.key).context("failed to decode ID")?;

        Ok((id, state))
    })
}

fn is_not_found(res: &AbciQuery) -> bool {
    res.code.value() == ExitCode::USR_NOT_FOUND.value()
}

fn parse_deliver_tx(res: AbciQuery) -> anyhow::Result<DeliverTx> {
    let bz: Vec<u8> =
        fvm_ipld_encoding::from_slice(&res.value).context("failed to decode IPLD as bytes")?;

    let deliver_tx = ResponseDeliverTx::decode(bz.as_ref())
        .context("failed to deserialize ResponseDeliverTx from proto bytes")?;

    let mut deliver_tx = DeliverTx::try_from(deliver_tx)
        .context("failed to create DeliverTx from proto response")?;

    // Mimic the Base64 encoding of the value that Tendermint does.
    deliver_tx.data = encode_data(&deliver_tx.data);

    Ok(deliver_tx)
}

#[cfg(test)]
mod tests {
    use tendermint_rpc::endpoint::abci_query::AbciQuery;

    use super::parse_deliver_tx;

    #[test]
    fn parse_call_query_response() {
        // Value extracted from a log captured in an issue.
        let response = "{\"code\":0,\"log\":\"\",\"info\":\"\",\"index\":\"0\",\"key\":null,\"value\":\"mNwIGCESARhAGCIYVxhtGGUYcxhzGGEYZxhlGCAYZhhhGGkYbBhlGGQYIBh3GGkYdBhoGCAYYhhhGGMYaxh0GHIYYRhjGGUYOgoYMBgwGDoYIBh0GDAYMRgxGDkYIBgoGG0YZRh0GGgYbxhkGCAYMxg4GDQYNBg0GDUYMBg4GDMYNxgpGCAYLRgtGCAYYxhvGG4YdBhyGGEYYxh0GCAYchhlGHYYZRhyGHQYZRhkGCAYKBgzGDMYKQoYMBiuGK0YpAEYOhh3CgcYbRhlGHMYcxhhGGcYZRIYNgoEGGYYchhvGG0SGCwYdBg0GDEYMBhmGGEYYRhhGGEYYRhhGGEYYRhhGGEYYRhhGGEYYRhhGGEYYRhhGGEYYRhhGGEYYRhhGGEYYRhhGGEYYRhhGGEYYRhvGG4YYxg2GGkYahhpGBgBEhg0CgIYdBhvEhgsGHQYNBgxGDAYZhg3GG8YNhh3GHYYNBhtGGgYaRg2GG0YdRgzGHgYZhhpGGYYdhhmGGcYbxhyGGIYYRhtGDUYbhhwGGcYbBhpGG0YNBhkGHkYdRh2GGkYaRgYAQ==\",\"proofOps\":null,\"height\":\"6148\",\"codespace\":\"\"}";
        let query = serde_json::from_str::<AbciQuery>(response).expect("failed to parse AbciQuery");
        let deliver_tx = parse_deliver_tx(query).expect("failed to parse DeliverTx");
        assert!(deliver_tx.code.is_err());
        assert_eq!(deliver_tx.info, "message failed with backtrace:\n00: t0119 (method 3844450837) -- contract reverted (33)\n");
    }
}
