// Copyright 2024 ADM Contributors
// SPDX-License-Identifier: Apache-2.0, MIT

use anyhow::anyhow;
use cid::Cid;
use clap::{Args, Subcommand};
use clap_stdin::FileOrStdin;
use fendermint_actor_machine::WriteAccess;
use fendermint_actor_objectstore::{GetParams, ListParams, Object, ObjectKind, PutParams};
use fendermint_vm_message::query::FvmQueryHeight;
use fvm_ipld_encoding::serde_bytes::ByteBuf;
use fvm_shared::address::Address;
use serde_json::json;
use tendermint_rpc::HttpClient;
use tokio::io::{self, AsyncReadExt, AsyncWriteExt};

use adm_provider::{json_rpc::JsonRpcProvider, BroadcastMode};
use adm_sdk::machine::{objectstore::ObjectStore, Machine};

use crate::{get_signer, print_json, Cli};

#[derive(Clone, Debug, Args)]
pub struct ObjectstoreArgs {
    #[command(subcommand)]
    command: ObjectstoreCommands,
}

#[derive(Clone, Debug, Subcommand)]
enum ObjectstoreCommands {
    Create(ObjectstoreCreateArgs),
    Put(ObjectstorePutArgs),
    Get(ObjectstoreGetArgs),
    List(ObjectstoreListArgs),
}

#[derive(Clone, Debug, Args)]
struct ObjectstoreCreateArgs {
    #[arg(long, default_value_t = false)]
    public_write: bool,
}

#[derive(Clone, Debug, Args)]
struct ObjectstorePutArgs {
    #[arg(short, long)]
    address: Address,
    #[arg(short, long)]
    key: String,
    #[clap(default_value = "-")]
    input: FileOrStdin,
}

#[derive(Clone, Debug, Args)]
struct ObjectstoreGetArgs {
    #[arg(short, long)]
    address: Address,
    #[arg(short, long)]
    key: String,
}

#[derive(Clone, Debug, Args)]
struct ObjectstoreListArgs {
    #[arg(short, long)]
    address: Address,
    #[arg(short, long, default_value = "")]
    prefix: String,
    #[arg(short, long, default_value = "/")]
    delimiter: String,
    #[arg(short, long, default_value_t = 0)]
    offset: u64,
    #[arg(short, long, default_value_t = 0)]
    limit: u64,
}

pub async fn handle_objectstore(cli: Cli, args: &ObjectstoreArgs) -> anyhow::Result<()> {
    let provider = JsonRpcProvider::new_http(cli.rpc_url, None)?;

    match &args.command {
        ObjectstoreCommands::Create(args) => {
            let mut signer = get_signer(&provider, cli.wallet_pk, cli.chain_name).await?;
            let write_access = if args.public_write {
                WriteAccess::Public
            } else {
                WriteAccess::OnlyOwner
            };
            let (store, tx) =
                ObjectStore::new(&provider, &mut signer, write_access, Default::default()).await?;

            print_json(&json!({"address": store.address().to_string(), "tx": &tx}))
        }
        ObjectstoreCommands::Put(args) => {
            let mut signer = get_signer(&provider, cli.wallet_pk, cli.chain_name).await?;
            let machine = ObjectStore::<HttpClient>::attach(args.address);
            let key = args.key.as_str();
            let mut reader = args.input.into_async_reader().await?;
            // TODO: Pipe input to ObjectAPI if greater than MAX_INTERNAL_OBJECT_SIZE
            // TODO: Input size in unknown, we could read_exact up to limit, and if there's
            // TODO: still more to read, rewind reader and upload to ObjectApi
            // let mut buf = vec![0; 8];
            // reader.read_exact(&mut buf).await?;

            // TODO: Below is a start on how we'd upload to ObjectApi if input is big enough
            // TODO: This doesn't quite work with plain AsyncRead. All the examples use File, which must be Send.
            // TODO: We might have to use tokio::sync::mpsc
            // TODO: Here's a discussion about showing a progress bar: https://github.com/seanmonstar/reqwest/issues/879
            // let stream = FramedRead::new(&mut reader, BytesCodec::new());
            // let body = Body::wrap_stream(stream);
            // let client = reqwest::Client::new();
            // client
            //     .post(&format!(
            //         "{}/v1/objects/{}",
            //         objects_url,
            //         args.address.to_string()
            //     ))
            //     .body(body)
            //     .send()
            //     .await?;

            // Just creating internal objects for now
            let mut buf = Vec::new();
            reader.read_to_end(&mut buf).await?;

            let tx = machine
                .put(
                    &provider,
                    &mut signer,
                    PutParams {
                        key: key.as_bytes().to_vec(),
                        kind: ObjectKind::Internal(ByteBuf(buf)),
                        overwrite: true, // TODO: make an arg
                    },
                    BroadcastMode::Commit,
                    Default::default(),
                )
                .await?;

            print_json(&tx)
        }
        ObjectstoreCommands::Get(args) => {
            // TODO: Handle range requests
            // TODO: Show progress bar?
            let machine = ObjectStore::<HttpClient>::attach(args.address);
            let key = args.key.as_str();
            let object = machine
                .get(
                    &provider,
                    GetParams {
                        key: key.as_bytes().to_vec(),
                    },
                    FvmQueryHeight::Committed,
                )
                .await?;

            if let Some(object) = object {
                match object {
                    Object::Internal(buf) => {
                        let mut stdout = io::stdout();
                        stdout.write_all(&buf.0).await?;
                        Ok(())
                    }
                    Object::External((buf, resolved)) => {
                        let cid = Cid::try_from(buf.0)?;
                        let cid = cid.to_string();
                        if !resolved {
                            return Err(anyhow!("object is not resolved"));
                        }

                        print_json(&json!({"cid": cid}))
                        // TODO: Get cid from ObjectApi
                    }
                }
            } else {
                Err(anyhow!("object not found for key '{}'", key))
            }
        }
        ObjectstoreCommands::List(args) => {
            let machine = ObjectStore::<HttpClient>::attach(args.address);
            let prefix = args.prefix.as_str();
            let delimiter = args.delimiter.as_str();
            let list = machine
                .list(
                    &provider,
                    ListParams {
                        prefix: prefix.as_bytes().to_vec(),
                        delimiter: delimiter.as_bytes().to_vec(),
                        offset: args.offset,
                        limit: args.limit,
                    },
                    FvmQueryHeight::Committed,
                )
                .await?;
            // TODO: ObjectList doesn't need to return as an Option. We can change this in the actor.

            if let Some(list) = list {
                print_json(&list)
            } else {
                Err(anyhow!("object list not found for prefix '{}'", prefix))
            }
        }
    }
}
