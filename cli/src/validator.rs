// Copyright 2024 Hoku Contributors
// SPDX-License-Identifier: Apache-2.0, MIT

use std::str::FromStr;

//use crate::commands::validator::batch_claim::{BatchClaim, BatchClaimArgs};
//use crate::commands::validator::list::{ListActivities, ListActivitiesArgs};
use crate::print_json;
use anyhow::{anyhow, Result};
use std::borrow::Borrow;
//use crate::{CommandLineHandler, GlobalArguments};
use clap::{Args, Subcommand};
use ethers::middleware::Middleware;
use ethers::prelude::{Signer, SignerMiddleware};
use ethers::providers::{Authorization, Http, Provider};
use ethers::signers::{LocalWallet, Wallet};
use ethers::types::{Eip1559TransactionRequest, ValueOrArray, H256, U256};
use ethers::{
    core::types::{transaction::eip2718::TypedTransaction, BlockId},
    providers::{MiddlewareError, PendingTransaction, ProviderError},
};
use ethers_contract::{ContractError, EthLogDecode, LogMeta};
use hoku_provider::fvm_shared::{address::Address, address::Payload, clock::ChainEpoch};
use hoku_sdk::network::NetworkConfig;
use ipc_actors_abis::{
    checkpointing_facet, gateway_getter_facet, gateway_manager_facet, gateway_messenger_facet,
    lib_gateway, lib_quorum, lib_staking_change_log, register_subnet_facet,
    subnet_actor_activity_facet::{self, ValidatorClaim},
    subnet_actor_checkpointing_facet, subnet_actor_getter_facet, subnet_actor_manager_facet,
    subnet_actor_reward_facet,
};
use ipc_api::merkle::MerkleGen;
use ipc_api::subnet_id::SubnetID;
use ipc_api::{
    checkpoint::{
        consensus::ValidatorData, BottomUpCheckpoint, BottomUpCheckpointBundle, QuorumReachedEvent,
        Signature, VALIDATOR_REWARD_FIELDS,
    },
    ethers_address_to_fil_address,
};
use ipc_provider::{config, manager::EthSubnetManager, IpcProvider};
use reqwest::Client;
use serde_json::json;
use std::sync::{Arc, RwLock};
//use fvm_shared::{address::Address, clock::ChainEpoch};
//use fvm_shared::address::{Address, Payload, Chain};
use hex;
//use crate::{
//f64_to_token_amount, get_ipc_provider, require_fil_addr_from_str, CommandLineHandler,
//GlobalArguments,
//};
use super::gas_estimator_middleware::Eip1559GasEstimatorMiddleware;
use ethers::prelude::k256::ecdsa::SigningKey;
pub type SignerWithFeeEstimatorMiddleware =
    Eip1559GasEstimatorMiddleware<SignerMiddleware<Provider<Http>, Wallet<SigningKey>>>;

use url::Url;

use hoku_signer::EthAddress;
/*use hoku_signer::{
    key::{random_secretkey, SecretKey},
    AccountKind, EthAddress, Signer, SubnetID, Void, Wallet,
};*/

#[derive(Clone, Debug, Args)]
pub(crate) struct ValidatorArgs {
    #[command(subcommand)]
    command: Commands,
}

/*impl ValidatorArgs {
    pub async fn handle(&self, global: &GlobalArguments) -> anyhow::Result<()> {
        match &self.command {
            Commands::BatchClaim(args) => BatchClaim::handle(global, args).await,
            //Commands::ListValidatorActivities(args) => ListActivities::handle(global, args).await,
        }
    }
}*/

#[derive(Clone, Debug, Subcommand)]
pub(crate) enum Commands {
    BatchClaim(BatchClaimArgs),
    //ListValidatorActivities(ListActivitiesArgs),
}

#[derive(Clone, Debug, Args)]
pub(crate) struct BatchClaimArgs {
    #[arg(long, help = "The JSON RPC server url for ipc agent")]
    pub validator: String,
    #[arg(long, help = "The checkpoint height to claim from")]
    pub from: ChainEpoch,
    #[arg(long, help = "The checkpoint height to claim to")]
    pub to: ChainEpoch,
}

/// Validator commands handler.
pub async fn handle_validator(cfg: NetworkConfig, args: &ValidatorArgs) -> anyhow::Result<()> {
    //let provider = JsonRpcProvider::new_http(cfg.rpc_url, None, None)?;

    let evm_subnet = config::subnet::EVMSubnet {
        provider_http: Url::parse("http://localhost:8545")?,
        //provider_http: Url::parse(cfg.evm_rpc_url)?,
        provider_timeout: None,
        auth_token: None,
        registry_addr: cfg.evm_gateway_address,
        gateway_addr: cfg.evm_gateway_address,
    };

    print!(">>> provider_http: {}\n", evm_subnet.provider_http);
    print!(">>> subnet_id: {}\n", cfg.subnet_id.inner());
    print!(
        ">>> subnet_id.parent(): {:?}\n",
        cfg.subnet_id.parent().map(|x| x.inner())
    );
    print!(">>> evm_gateway_address: {}\n", cfg.evm_gateway_address);

    let subnet = config::Subnet {
        //id: cfg.subnet_id.parent()?.inner(),
        id: cfg.subnet_id.inner(),
        config: config::subnet::SubnetConfig::Fevm(evm_subnet),
    };

    print!(">>> subnet: {:?}\n", subnet);

    //let ipc_provider = IpcProvider::new_with_subnet(None, subnet)?;

    match &args.command {
        Commands::BatchClaim(args) => {
            print!(">>> validator str: {}\n", args.validator);

            let bytes = hex::decode(&args.validator[2..]).expect("Invalid hex string");
            let eth_addr = EthAddress(bytes.try_into().expect("Wrong length"));

            let validator = Address::from(eth_addr);

            print!(">>> validator: {}\n", validator);

            //let child_manager = EthSubnetManager::from_subnet_with_wallet_store(&subnet, None)?;

            print!(">>> child_manager\n");

            //let claims = child_manager
            //.query_reward_claims(&validator, args.from, args.to)
            //.await?;
            let claims = query_reward_claims(&validator, args.from, args.to, &subnet).await?;

            print!(">>> claims: {:?}\n", claims);

            let parent = cfg
                .parent_subnet_config()
                .ok_or_else(|| anyhow!("no parent found"))?;

            print!(">>> parent: {:?}\n", parent);

            let parent_evm_subnet = config::subnet::EVMSubnet {
                provider_http: parent.provider_http,
                provider_timeout: None,
                auth_token: None,
                registry_addr: parent.registry_addr,
                gateway_addr: parent.gateway_addr,
            };
            let parent_subnet = config::Subnet {
                id: subnet
                    .id
                    .parent()
                    .ok_or_else(|| anyhow!("no parent found"))?,
                config: config::subnet::SubnetConfig::Fevm(parent_evm_subnet),
            };

            print!(">>> parent_subnet: {:?}\n", parent_subnet);

            //let parent_manager =
            //EthSubnetManager::from_subnet_with_wallet_store(&parent_subnet, None)?;

            print!(">>> parent_manager\n");

            batch_subnet_claim(&validator, &subnet.id, &subnet.id, claims, &subnet).await?;

            print_json(&json!("rewards claimed"))
        } /*Commands::ListValidatorActivities(args) => {
              let res = ListActivities::handle(global, args).await?;
              print_json(&json!(res))
          }*/
    }
}

fn create_provider(subnet: &config::Subnet) -> Result<Provider<Http>> {
    let client = Client::builder();
    let client = client.build()?;

    let url = subnet.rpc_http().clone();
    let provider = Http::new_with_client(url, client);
    Ok(Provider::new(provider))
}

async fn query_reward_claims(
    validator_addr: &Address,
    from_checkpoint: ChainEpoch,
    to_checkpoint: ChainEpoch,
    subnet: &config::Subnet,
) -> Result<Vec<(u64, ValidatorClaim)>> {
    let provider = create_provider(subnet)?;

    let config::subnet::SubnetConfig::Fevm(config) = &subnet.config;
    let gateway_address = payload_to_evm_address(config.gateway_addr.payload())?;
    let contract =
        checkpointing_facet::CheckpointingFacet::new(gateway_address, Arc::new(provider));

    let ev = contract
        .event::<checkpointing_facet::ActivityRollupRecordedFilter>()
        .from_block(from_checkpoint as u64)
        .to_block(to_checkpoint as u64)
        .address(ValueOrArray::Value(contract.address()));

    let validator_eth_addr = payload_to_evm_address(validator_addr.payload())?;

    let mut claims = vec![];
    for (event, meta) in query_with_meta(ev, contract.client()).await? {
        tracing::debug!(
            "found activity bundle published at height: {}",
            meta.block_number
        );

        // Check if we have claims for this validator in this block.
        let our_data = event
            .rollup
            .consensus
            .data
            .iter()
            .find(|v| v.validator == validator_eth_addr);

        // If we don't, skip this block.
        let Some(data) = our_data else {
            tracing::info!(
                "target validator address has no reward claims in epoch {}",
                meta.block_number
            );
            continue;
        };

        let proof = gen_merkle_proof(&event.rollup.consensus.data, data)?;

        // Construct the claim and add it to the list.
        let claim = ValidatorClaim {
            // Even though it's the same struct but still need to do a mapping due to
            // different crate from ethers-rs
            data: subnet_actor_activity_facet::ValidatorData {
                validator: data.validator,
                blocks_committed: data.blocks_committed,
            },
            proof: proof.into_iter().map(|v| v.into()).collect(),
        };
        claims.push((event.checkpoint_height, claim));
    }

    Ok(claims)
}

fn payload_to_evm_address(payload: &Payload) -> anyhow::Result<ethers::types::Address> {
    match payload {
        Payload::Delegated(delegated) => {
            let slice = delegated.subaddress();
            Ok(ethers::types::Address::from_slice(&slice[0..20]))
        }
        _ => Err(anyhow!("address provided is not delegated")),
    }
}

async fn query_with_meta<B, M, D>(
    event: ethers::contract::Event<B, M, D>,
    client: B,
) -> Result<Vec<(D, LogMeta)>, ContractError<M>>
where
    B: Borrow<M>,
    M: Middleware,
    D: EthLogDecode,
{
    let logs = client
        .borrow()
        .get_logs(&event.filter)
        .await
        .map_err(ContractError::from_middleware_error)?;

    let events = logs
        .into_iter()
        .filter(|l| !l.removed.unwrap_or_default())
        .map(|log| {
            let meta = LogMeta::from(&log);
            let event = ethers::contract::parse_log::<D>(log)?;
            Ok((event, meta))
        })
        .collect::<Result<_, ContractError<M>>>()?;

    Ok(events)
}

fn gen_merkle_proof(
    validator_data: &[checkpointing_facet::ValidatorData],
    validator: &checkpointing_facet::ValidatorData,
) -> anyhow::Result<Vec<H256>> {
    // Utilty function to pack validator data into a vector of strings for proof generation.
    let pack_validator_data = |v: &checkpointing_facet::ValidatorData| {
        vec![format!("{:?}", v.validator), v.blocks_committed.to_string()]
    };

    let leaves = order_validator_data(validator_data)?;
    let tree = MerkleGen::new(pack_validator_data, &leaves, &VALIDATOR_REWARD_FIELDS)?;

    tree.get_proof(validator)
}

fn order_validator_data(
    validator_data: &[checkpointing_facet::ValidatorData],
) -> anyhow::Result<Vec<checkpointing_facet::ValidatorData>> {
    let mut mapped = validator_data
        .iter()
        .map(|a| ethers_address_to_fil_address(&a.validator).map(|v| (v, a.blocks_committed)))
        .collect::<Result<Vec<_>, _>>()?;

    mapped.sort_by(|a, b| {
        let cmp = a.0.cmp(&b.0);
        if cmp.is_eq() {
            // Address will be unique, do this just in case equal
            a.1.cmp(&b.1)
        } else {
            cmp
        }
    });

    let back_to_eth = |(fvm_addr, blocks): (Address, u64)| {
        payload_to_evm_address(fvm_addr.payload()).map(|v| checkpointing_facet::ValidatorData {
            validator: v,
            blocks_committed: blocks,
        })
    };
    mapped
        .into_iter()
        .map(back_to_eth)
        .collect::<Result<Vec<_>, _>>()
}

async fn batch_subnet_claim(
    submitter: &Address,
    reward_claim_subnet: &SubnetID,
    reward_origin_subnet: &SubnetID,
    claims: Vec<(u64, ValidatorClaim)>,
    subnet: &config::Subnet,
) -> Result<()> {
    let signer = Arc::new(get_signer_with_fee_estimator(submitter, subnet)?);
    print!(">>> signer: {:?}\n", signer);
    let contract = subnet_actor_activity_facet::SubnetActorActivityFacet::new(
        contract_address_from_subnet(reward_claim_subnet)?,
        signer.clone(),
    );
    print!(">>> contract: {:?}\n", contract);

    // separate the Vec of tuples claims into two Vecs of Height and Claim
    let (heights, claims): (Vec<u64>, Vec<ValidatorClaim>) = claims.into_iter().unzip();
    print!(">>> heights: {:?}\n", heights);
    print!(">>> claims: {:?}\n", claims);

    let call = {
        let call = contract.batch_subnet_claim(reward_origin_subnet.try_into()?, heights, claims);
        print!(">>> call: {:?}\n", call);
        extend_call_with_pending_block(call).await?
    };

    call.send().await?;

    Ok(())
}

fn get_signer_with_fee_estimator(
    addr: &Address,
    subnet: &config::Subnet,
) -> Result<SignerWithFeeEstimatorMiddleware> {
    // convert to its underlying eth address
    //let addr = payload_to_evm_address(addr.payload())?;
    //let keystore = self.keystore()?;
    //let keystore = keystore.read().unwrap();
    //let private_key = keystore
    //.get(&addr.into())?
    //.ok_or_else(|| anyhow!("address {addr:} does not have private key in key store"))?;

    // read the private key from HOKU_PRIVATE_KEY env var
    let private_key = &std::env::var("HOKU_PRIVATE_KEY").expect("HOKU_PRIVATE_KEY env var not set");
    print!(">>> private_key: {}\n", private_key);
    let wallet = LocalWallet::from_str(private_key)?.with_chain_id(subnet.id.chain_id());
    //let wallet =
    //LocalWallet::from_bytes(private_key.private_key())?.with_chain_id(subnet.id.chain_id());
    print!(">>> wallet: {:?}\n", wallet);

    let provider = create_provider(subnet)?;
    let signer = SignerMiddleware::new(provider, wallet);
    Ok(Eip1559GasEstimatorMiddleware::new(signer))
}

pub(crate) fn contract_address_from_subnet(subnet: &SubnetID) -> Result<ethers::types::Address> {
    let children = subnet.children();
    let ipc_addr = children
        .last()
        .ok_or_else(|| anyhow!("{subnet:} has no child"))?;

    payload_to_evm_address(ipc_addr.payload())
}

pub(crate) async fn extend_call_with_pending_block<B, D, M>(
    call: ethers_contract::FunctionCall<B, D, M>,
) -> Result<ethers_contract::FunctionCall<B, D, M>>
where
    B: std::borrow::Borrow<D>,
    M: ethers::abi::Detokenize,
{
    Ok(call.block(ethers::types::BlockNumber::Pending))
}
