// Copyright 2024 Hoku Contributors
// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

use anyhow::{anyhow, Context};
use async_trait::async_trait;
use cid::Cid;
use prost::Message as ProstMessage;
use serde::Serialize;
use tendermint::{abci::response::DeliverTx, block::Height};
use tendermint_proto::abci::ResponseDeliverTx;
use tendermint_rpc::endpoint::abci_query::AbciQuery;

use crate::fvm_shared::{address::Address, error::ExitCode, message::Message, ActorID};
use crate::response::encode_data;

pub use fendermint_vm_message::query::{
    ActorState, BuiltinActors, FvmQuery, FvmQueryHeight, GasEstimate, StateParams,
};

/// The parsed query response.
#[derive(Debug, Clone, Serialize)]
pub struct QueryResponse<T> {
    /// The height at which the query was performed.
    pub height: Height,
    /// The parsed response value.
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
    async fn estimate_gas_limit(
        &self,
        mut message: Message,
        height: FvmQueryHeight,
    ) -> anyhow::Result<u64> {
        if message.gas_limit != 0 {
            return Ok(message.gas_limit);
        }

        // Using a sequence of 0 so estimation doesn't get tripped over by nonce mismatch.
        message.sequence = 0;

        let res = self
            .query(FvmQuery::EstimateGas(Box::new(message)), height)
            .await?;
        let estimate: GasEstimate = extract(res, |res| {
            fvm_ipld_encoding::from_slice(&res.value)
                .context("failed to decode GasEstimate from query")
        })?;

        Ok(estimate.gas_limit)
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

/// Extract some value from the query result, unless it's not found or another error.
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
            "query returned non-zero exit code: {}; {}",
            res.code.value(),
            res.info,
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
