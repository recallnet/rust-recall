// Copyright 2024 Hoku Contributors
// SPDX-License-Identifier: Apache-2.0, MIT

use std::str::FromStr;

//use crate::commands::validator::batch_claim::{BatchClaim, BatchClaimArgs};
//use crate::commands::validator::list::{ListActivities, ListActivitiesArgs};
use crate::print_json;
//use crate::{CommandLineHandler, GlobalArguments};
use clap::{Args, Subcommand};
use hoku_provider::fvm_shared::{address::Address, clock::ChainEpoch};
use hoku_sdk::network::NetworkConfig;
//use fvm_shared::{address::Address, clock::ChainEpoch};
//use ipc_api::subnet_id::SubnetID;
use hex;
use ipc_provider::{config, IpcProvider};
use serde_json::json;
//use crate::{
//f64_to_token_amount, get_ipc_provider, require_fil_addr_from_str, CommandLineHandler,
//GlobalArguments,
//};

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

    let ipc_provider = IpcProvider::new_with_subnet(None, subnet)?;

    match &args.command {
        Commands::BatchClaim(args) => {
            print!(">>> validator str: {}\n", args.validator);

            let bytes = hex::decode(&args.validator[2..]).expect("Invalid hex string");
            let eth_addr = EthAddress(bytes.try_into().expect("Wrong length"));
            //let mut bbb = [0u8; 20];
            //bbb.copy_from_slice(&hex::decode(&args.validator[2..]).expect("Invalid hex string"));
            //let eth_addr = EthAddress(bbb);

            //let eth_address = EthAddress{args.validator.as_bytes().to_owned()};

            let validator = Address::from(eth_addr);
            //cfg.subnet_id

            print!(">>> validator: {}\n", validator);

            let res = ipc_provider
                .batch_subnet_claim(
                    &cfg.subnet_id.inner(),
                    &cfg.subnet_id.inner(),
                    args.from,
                    args.to,
                    &validator,
                )
                .await;

            print!(">>> res: {:?}\n", res);

            res?;

            //let res = BatchClaim::handle(global, args).await?;
            print_json(&json!("rewards claimed"))
        } /*Commands::ListValidatorActivities(args) => {
              let res = ListActivities::handle(global, args).await?;
              print_json(&json!(res))
          }*/
    }
}
