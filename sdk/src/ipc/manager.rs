// Copyright 2024 Hoku Contributors
// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

use std::sync::Arc;
use std::time::Duration;

use anyhow::anyhow;
use ethers::prelude::abigen;
use ethers::{
    core::k256::ecdsa::SigningKey,
    middleware::{Middleware, SignerMiddleware},
    prelude::{
        Authorization, Eip1559TransactionRequest, Http, LocalWallet, Provider, Signer as EthSigner,
        TransactionReceipt, Wallet, I256, U256,
    },
    types::transaction::eip2718::TypedTransaction,
};
use ethers_contract::ContractCall;
use fvm_shared::{address::Address, econ::TokenAmount};
use gateway_manager_facet::{FvmAddress, GatewayManagerFacet, SubnetID as GatewaySubnetID};
use ipc_actors_abis::gateway_manager_facet;
use ipc_api::evm::{fil_to_eth_amount, payload_to_evm_address};
use num_traits::ToPrimitive;
use reqwest::{header::HeaderValue, Client};

use hoku_signer::Signer;

use crate::ipc::subnet::EVMSubnet;

type DefaultSignerMiddleware = SignerMiddleware<Provider<Http>, Wallet<SigningKey>>;

/// Default polling time used by the Ethers provider to check for pending
/// transactions and events. Default is 7, and for our child subnets we
/// can reduce it to the block time (or potentially less)
const ETH_PROVIDER_POLLING_TIME: Duration = Duration::from_secs(1);
/// Maximum number of retries to fetch a transaction receipt.
/// The number of retries should ensure that for the block time
/// of the network the number of retries considering the polling
/// time above waits enough time to get the transaction receipt.
/// We currently support a low polling time and high number of
/// retries so these numbers accommodate fast subnets with slow
/// roots (like Calibration and mainnet).
const TRANSACTION_RECEIPT_RETRIES: usize = 200;

// Generate ABI for `approval` method on ERC20
abigen!(
    IERC20,
    r#"[{"inputs":[{"internalType":"address","name":"spender","type":"address"},{"internalType":"uint256","name":"amount","type":"uint256"}],"name":"approve","outputs":[{"internalType":"bool","name":"","type":"bool"}],"stateMutability":"nonpayable","type":"function"}]"#
);

/// Returns an Ethereum provider for the given subnet configuration.
fn get_eth_provider(subnet: &EVMSubnet) -> anyhow::Result<Provider<Http>> {
    let url = subnet.provider_http.clone();
    let auth_token = subnet.auth_token.clone();

    let mut client = Client::builder();
    if let Some(auth_token) = auth_token {
        let auth = Authorization::Bearer(auth_token);
        let mut auth_value = HeaderValue::from_str(&auth.to_string())?;
        auth_value.set_sensitive(true);
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(reqwest::header::AUTHORIZATION, auth_value);
        client = client.default_headers(headers);
    }
    if let Some(timeout) = subnet.provider_timeout {
        client = client.timeout(timeout);
    }
    let client = client.build()?;

    let provider = Http::new_with_client(url, client);
    let mut provider = Provider::new(provider);
    provider.set_interval(ETH_PROVIDER_POLLING_TIME);

    Ok(provider)
}

/// Returns an Ethereum signer using [`Signer`] for the given subnet configuration.
fn get_eth_signer(
    signer: &impl Signer,
    subnet: &EVMSubnet,
) -> anyhow::Result<DefaultSignerMiddleware> {
    let provider = get_eth_provider(subnet)?;

    let secret_key = match signer.secret_key() {
        Some(sk) => sk,
        None => return Err(anyhow!("failed to get secret key from signer")),
    };
    let subnet_id = match signer.subnet_id() {
        Some(subnet_id) => subnet_id,
        None => return Err(anyhow!("failed to get subnet ID from signer"))?,
    };
    let chain_id = subnet_id.chain_id();

    let sk = secret_key.serialize();
    let wallet = LocalWallet::from_bytes(sk.as_slice())?.with_chain_id(chain_id);

    Ok(SignerMiddleware::new(provider, wallet))
}

/// Returns an interface to a [`GatewayManagerFacet`]
/// using [`Signer`] for the given subnet configuration.
fn get_gateway(
    signer: &impl Signer,
    subnet: &EVMSubnet,
) -> anyhow::Result<Box<GatewayManagerFacet<DefaultSignerMiddleware>>> {
    let address = payload_to_evm_address(subnet.gateway_addr.payload())?;
    let signer = get_eth_signer(signer, subnet)?;

    Ok(Box::new(GatewayManagerFacet::new(
        address,
        Arc::new(signer),
    )))
}

/// Returns an interface to the [`IERC20`] contract
/// using [`Signer`] for the given subnet configuration.
fn get_supply_source(
    signer: &impl Signer,
    subnet: &EVMSubnet,
) -> anyhow::Result<Box<IERC20<DefaultSignerMiddleware>>> {
    let supply_source = match subnet.supply_source {
        Some(addr) => addr,
        None => return Err(anyhow!("supply source is not configured for parent subnet")),
    };
    let address = payload_to_evm_address(supply_source.payload())?;
    let signer = get_eth_signer(signer, subnet)?;

    Ok(Box::new(IERC20::new(address, Arc::new(signer))))
}

/// A static wrapper around common EVM subnet methods.
pub struct EvmManager {}

impl EvmManager {
    /// Get the balance of an account in a subnet.
    pub async fn balance(address: Address, subnet: EVMSubnet) -> anyhow::Result<TokenAmount> {
        let provider = get_eth_provider(&subnet)?;
        let balance = provider
            .get_balance(payload_to_evm_address(address.payload())?, None)
            .await?;
        Ok(TokenAmount::from_atto(balance.as_u128()))
    }

    /// Approve the gateway to spend funds on behalf of the user.
    /// This is required to [`deposit`] work.
    pub async fn approve_gateway(
        signer: &impl Signer,
        subnet: EVMSubnet,
        amount: TokenAmount,
    ) -> anyhow::Result<TransactionReceipt> {
        let gateway = get_gateway(signer, &subnet)?;
        let supply_source = get_supply_source(signer, &subnet)?;
        let value = amount
            .atto()
            .to_u128()
            .ok_or_else(|| anyhow!("invalid value to fund"))?;

        let call = supply_source.approve(gateway.address(), value.into());

        client_send(supply_source.client(), call).await
    }

    /// Deposit funds into a subnet.
    pub async fn deposit(
        signer: &impl Signer,
        to: Address,
        subnet: EVMSubnet,
        amount: TokenAmount,
    ) -> anyhow::Result<TransactionReceipt> {
        let gateway = get_gateway(signer, &subnet)?;
        let subnet_id = GatewaySubnetID::try_from(&subnet.id.inner())?;

        let value = amount
            .atto()
            .to_u128()
            .ok_or_else(|| anyhow!("invalid value to fund"))?;

        let call = gateway.fund_with_token(subnet_id, FvmAddress::try_from(to)?, value.into());

        client_send(gateway.client(), call).await
    }

    /// Withdraw funds from a subnet.
    pub async fn withdraw(
        signer: &impl Signer,
        to: Address,
        subnet: EVMSubnet,
        amount: TokenAmount,
    ) -> anyhow::Result<TransactionReceipt> {
        let gateway = get_gateway(signer, &subnet)?;

        let value = amount
            .atto()
            .to_u128()
            .ok_or_else(|| anyhow!("invalid value to fund"))?;

        let mut call = gateway.release(FvmAddress::try_from(to)?);
        call.tx.set_value(value);

        client_send(gateway.client(), call).await
    }

    /// Transfer funds between two accounts in a subnet.
    pub async fn transfer(
        signer: &impl Signer,
        to: Address,
        subnet: EVMSubnet,
        amount: TokenAmount,
    ) -> anyhow::Result<TransactionReceipt> {
        let signer = Arc::new(get_eth_signer(signer, &subnet)?);

        let (fee, fee_cap) = premium_estimation(signer.clone()).await?;
        let tx = Eip1559TransactionRequest::new()
            .to(payload_to_evm_address(to.payload())?)
            .value(fil_to_eth_amount(&amount)?)
            .max_priority_fee_per_gas(fee)
            .max_fee_per_gas(fee_cap);

        let tx_pending = signer.send_transaction(tx, None).await?;
        tx_pending
            .await?
            .ok_or(anyhow!("transfer did not return receipt"))
    }
}

/// Sends a contract call with configured retries using the provided client.
async fn client_send<T: ethers::abi::Detokenize>(
    client: Arc<DefaultSignerMiddleware>,
    call: ContractCall<DefaultSignerMiddleware, T>,
) -> anyhow::Result<TransactionReceipt> {
    let call = call_with_premium_and_pending_block(client, call).await?;
    let tx = call.send().await?;
    match tx.retries(TRANSACTION_RECEIPT_RETRIES).await? {
        Some(receipt) => Ok(receipt),
        None => Err(anyhow!(
            "txn sent to network, but receipt cannot be obtained, please check scanner"
        )),
    }
}

/// Takes a `FunctionCall` input and returns a new instance with an estimated optimal `gas_premium`.
/// The function also uses the pending block number to help retrieve the latest nonce
/// via `get_transaction_count` with the `pending` parameter.
async fn call_with_premium_and_pending_block<B, D, M>(
    signer: Arc<DefaultSignerMiddleware>,
    mut call: ethers_contract::FunctionCall<B, D, M>,
) -> anyhow::Result<ethers_contract::FunctionCall<B, D, M>>
where
    B: std::borrow::Borrow<D>,
    M: ethers::abi::Detokenize,
{
    let (max_priority_fee_per_gas, max_fee_per_gas) = premium_estimation(signer).await?;
    let call_with_gas = match call.tx.clone() {
        TypedTransaction::Eip1559(mut tx) => {
            tx.max_fee_per_gas = Some(max_fee_per_gas);
            tx.max_priority_fee_per_gas = Some(max_priority_fee_per_gas);
            call.tx = TypedTransaction::Eip1559(tx);
            call
        }
        TypedTransaction::Legacy(mut tx) => {
            tx.gas_price = Some(max_fee_per_gas);
            call.tx = TypedTransaction::Legacy(tx);
            call
        }
        TypedTransaction::Eip2930(mut wrapped_tx) => {
            wrapped_tx.tx.gas_price = Some(max_fee_per_gas);
            call.tx = TypedTransaction::Eip2930(wrapped_tx);
            call
        }
    };

    Ok(call_with_gas.block(ethers::types::BlockNumber::Pending))
}

/// Returns an estimation of an optimal `gas_premium` and `gas_fee_cap`
/// for a transaction considering the average premium, base_fee and reward percentile from
/// past blocks
/// This is an adaptation of ethers' `eip1559_default_estimator`:
/// https://github.com/gakonst/ethers-rs/blob/5dcd3b7e754174448f9a8cbfc0523896609629f9/ethers-core/src/utils/mod.rs#L476
async fn premium_estimation(signer: Arc<DefaultSignerMiddleware>) -> anyhow::Result<(U256, U256)> {
    let base_fee_per_gas = signer
        .get_block(ethers::types::BlockNumber::Latest)
        .await?
        .ok_or_else(|| anyhow!("Latest block not found"))?
        .base_fee_per_gas
        .ok_or_else(|| anyhow!("EIP-1559 not activated"))?;

    let fee_history = signer
        .fee_history(
            ethers::utils::EIP1559_FEE_ESTIMATION_PAST_BLOCKS,
            ethers::types::BlockNumber::Latest,
            &[ethers::utils::EIP1559_FEE_ESTIMATION_REWARD_PERCENTILE],
        )
        .await?;

    let max_priority_fee_per_gas = estimate_priority_fee(fee_history.reward); //overestimate?
    let potential_max_fee = base_fee_surged(base_fee_per_gas);
    let max_fee_per_gas = if max_priority_fee_per_gas > potential_max_fee {
        max_priority_fee_per_gas + potential_max_fee
    } else {
        potential_max_fee
    };

    Ok((max_priority_fee_per_gas, max_fee_per_gas))
}

/// Implementation borrowed from
/// https://github.com/gakonst/ethers-rs/blob/ethers-v2.0.8/ethers-core/src/utils/mod.rs#L582
/// Refer to the implementation for unit tests
fn base_fee_surged(base_fee_per_gas: U256) -> U256 {
    if base_fee_per_gas <= U256::from(40_000_000_000u64) {
        base_fee_per_gas * 2
    } else if base_fee_per_gas <= U256::from(100_000_000_000u64) {
        base_fee_per_gas * 16 / 10
    } else if base_fee_per_gas <= U256::from(200_000_000_000u64) {
        base_fee_per_gas * 14 / 10
    } else {
        base_fee_per_gas * 12 / 10
    }
}

/// Implementation borrowed from
/// https://github.com/gakonst/ethers-rs/blob/ethers-v2.0.8/ethers-core/src/utils/mod.rs#L536
/// Refer to the implementation for unit tests
fn estimate_priority_fee(rewards: Vec<Vec<U256>>) -> U256 {
    let mut rewards: Vec<U256> = rewards
        .iter()
        .map(|r| r[0])
        .filter(|r| *r > U256::zero())
        .collect();
    if rewards.is_empty() {
        return U256::zero();
    }
    if rewards.len() == 1 {
        return rewards[0];
    }
    // Sort the rewards as we will eventually take the median.
    rewards.sort();

    // A copy of the same vector is created for convenience to calculate percentage change
    // between later fee values.
    let mut rewards_copy = rewards.clone();
    rewards_copy.rotate_left(1);

    let mut percentage_change: Vec<I256> = rewards
        .iter()
        .zip(rewards_copy.iter())
        .map(|(a, b)| {
            let a = I256::try_from(*a).expect("priority fee overflow");
            let b = I256::try_from(*b).expect("priority fee overflow");
            ((b - a) * 100) / a
        })
        .collect();
    percentage_change.pop();

    // Fetch the max of the percentage change, and that element's index.
    let max_change = percentage_change.iter().max().unwrap();
    let max_change_index = percentage_change
        .iter()
        .position(|&c| c == *max_change)
        .unwrap();

    // If we encountered a big change in fees at a certain position, then consider only
    // the values >= it.
    let values = if *max_change >= ethers::utils::EIP1559_FEE_ESTIMATION_THRESHOLD_MAX_CHANGE.into()
        && (max_change_index >= (rewards.len() / 2))
    {
        rewards[max_change_index..].to_vec()
    } else {
        rewards
    };

    // Return the median.
    values[values.len() / 2]
}
