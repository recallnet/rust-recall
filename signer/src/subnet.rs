// Copyright 2024 Hoku Contributors
// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

use anyhow::anyhow;
use std::fmt;
use std::fmt::Write;
use std::hash::Hasher;
use std::str::FromStr;

use fnv::FnvHasher;
use ipc_api::{error::Error, subnet_id::MAX_CHAIN_ID};

use hoku_provider::{fvm_shared::chainid::ChainID, util::parse_address};

fn hash(bytes: &[u8]) -> u64 {
    let mut hasher = FnvHasher::default();
    hasher.write(bytes);
    hasher.finish() % MAX_CHAIN_ID
}

/// Subnet ID wrapper that understands eth-addresses and doesn't use the current global address
/// protocol.
///
/// `Address::from_str` requires that `fvm_shared::address::network::Network` is set, which can't
/// be done in some situations like parsing command-line arguments.
///
/// This type tries to solve the sometimes overlapping and sometimes not overlapping uses of
/// subnet ID and chain ID that can be cumbersome to deal with independently, especially when
/// working with networks that may not have a parent.
#[derive(Clone, Debug)]
pub struct SubnetID {
    /// Value that is not valid [`ipc_api::subnet_id::SubnetID`], which is convenient for networks
    /// without a real parent.
    faux: String,
    /// A valid [`ipc_api::subnet_id::SubnetID`].
    real: ipc_api::subnet_id::SubnetID,

    /// Explicitely set chain ID. If not set the chain ID is computed as a hash from the subnet ID.
    explicit_chain_id: Option<ChainID>,
}

impl SubnetID {
    /// Returns the real subnet ID.
    pub fn inner(&self) -> ipc_api::subnet_id::SubnetID {
        self.real.clone()
    }

    /// Returns the parent subnet ID is it exists.
    pub fn parent(&self) -> anyhow::Result<Self> {
        if let Some(parent) = self.inner().parent() {
            Ok(Self {
                faux: Default::default(),
                real: parent,
                explicit_chain_id: None,
            })
        } else {
            Err(anyhow!("subnet has no parent"))
        }
    }

    pub fn with_chain_id(self, chain_id: ChainID) -> SubnetID {
        SubnetID {
            explicit_chain_id: Some(chain_id),
            ..self
        }
    }

    /// Returns the chain ID representation.
    pub fn chain_id(&self) -> ChainID {
        match self.explicit_chain_id {
            Some(chain_id) => chain_id,
            None => {
                if self.real.is_root() {
                    if self.faux.is_empty() {
                        ChainID::from(self.real.root_id())
                    } else {
                        ChainID::from(hash(self.faux.clone().as_bytes()))
                    }
                } else {
                    ChainID::from(self.real.chain_id())
                }
            }
        }
    }
}

impl fmt::Display for SubnetID {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.faux.is_empty() {
            let children_str =
                self.real
                    .children_as_ref()
                    .iter()
                    .fold(String::new(), |mut output, s| {
                        let _ = write!(output, "/{s}");
                        output
                    });
            write!(f, "/r{}{}", self.real.root_id(), children_str)
        } else {
            write!(f, "{}", self.faux)
        }
    }
}

impl FromStr for SubnetID {
    type Err = Error;
    fn from_str(id: &str) -> Result<Self, Error> {
        if !id.starts_with("/r") {
            return Ok(Self {
                faux: id.to_string(),
                real: Default::default(),
                explicit_chain_id: None,
            });
        }

        let segments: Vec<&str> = id.split('/').skip(1).collect();

        let root = segments[0][1..]
            .parse::<u64>()
            .map_err(|_| Error::InvalidID(id.into(), "invalid root ID".into()))?;

        let mut children = Vec::new();

        for addr in segments[1..].iter() {
            let addr = parse_address(addr).map_err(|e| {
                Error::InvalidID(id.into(), format!("invalid child address {addr}: {e}"))
            })?;
            children.push(addr);
        }

        Ok(Self {
            faux: Default::default(),
            real: ipc_api::subnet_id::SubnetID::new(root, children),
            explicit_chain_id: None,
        })
    }
}
