// Copyright 2024 Hoku Contributors
// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

use tendermint_rpc::Client;

use crate::object::ObjectProvider;
use crate::query::QueryProvider;
use crate::tx::TxProvider;

/// Provider capable of submitting queries and transactions.
pub trait Provider<C>: TendermintClient<C> + QueryProvider + TxProvider + ObjectProvider
where
    C: Client + Send + Sync,
{
}

/// Get to the underlying Tendermint client if necessary,
/// for example, to query the state of transactions.
pub trait TendermintClient<C>
where
    C: Client + Send + Sync,
{
    /// The underlying Tendermint client.
    fn underlying(&self) -> &C;
}
