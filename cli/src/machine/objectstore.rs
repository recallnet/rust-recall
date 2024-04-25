// Copyright 2024 ADM Contributors
// SPDX-License-Identifier: Apache-2.0, MIT

use anyhow::anyhow;
use async_tempfile::TempFile;
use cid::multihash::{Code, MultihashDigest};
use cid::Cid;
use clap::{Args, Parser, Subcommand};
use clap_stdin::FileOrStdin;
use console::Emoji;
use fendermint_actor_machine::WriteAccess;
use fendermint_actor_objectstore::{GetParams, ListParams, Object, ObjectKind, PutParams};
use fendermint_vm_core::chainid;
use fendermint_vm_message::query::FvmQueryHeight;
use fvm_ipld_encoding::serde_bytes::ByteBuf;
use fvm_shared::address::Address;
use indicatif::{ProgressBar, ProgressStyle};
use serde_json::json;
use std::sync::Arc;
use tendermint_rpc::HttpClient;
use tokio::fs::File;
use tokio::io::{self, AsyncRead, AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use tokio::sync::{
    Mutex,
    {mpsc, mpsc::Sender},
};
use tokio::task::LocalSet;

use adm_provider::{json_rpc::JsonRpcProvider, object::ObjectClient, BroadcastMode};
use adm_sdk::machine::{objectstore::generate_cid, objectstore::ObjectStore, Machine};

use crate::{get_signer, print_json, Cli};

const MAX_INTERNAL_OBJECT_LENGTH: u64 = 1024;

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

#[derive(Clone, Debug, Parser)]
struct ObjectstorePutArgs {
    /// Address of the object store    
    #[arg(short, long)]
    address: Address,
    /// Key of the object to put
    #[arg(short, long)]
    key: String,
    /// Overwrite the object if it already exists
    #[arg(short, long, action)]
    overwrite: bool,
    /// Input file path to upload
    #[clap(default_value = "-")]
    input: FileOrStdin,
}

#[derive(Clone, Debug, Args)]
struct ObjectstoreGetArgs {
    /// Address of the object store
    #[arg(short, long)]
    address: Address,
    /// Key of the object to get
    #[arg(short, long)]
    key: String,
    /// Output file path for download
    #[arg(short, long)]
    output: String,
}

#[derive(Clone, Debug, Args)]
struct ObjectstoreListArgs {
    /// Address of the object store
    #[arg(short, long)]
    address: Address,
    /// Prefix to filter objects
    #[arg(short, long, default_value = "")]
    prefix: String,
    /// Delimiter to filter objects
    #[arg(short, long, default_value = "/")]
    delimiter: String,
    /// Offset to start listing objects
    #[arg(short, long, default_value_t = 0)]
    offset: u64,
    /// Limit to list objects
    #[arg(short, long, default_value_t = 0)]
    limit: u64,
}

pub async fn handle_objectstore(cli: Cli, args: &ObjectstoreArgs) -> anyhow::Result<()> {
    let provider = JsonRpcProvider::new_http(cli.rpc_url.clone(), None)?;
    let chain_id = chainid::from_str_hashed(&cli.chain_name)?;
    let object_client = ObjectClient::new(cli.object_api_url, u64::from(chain_id));

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
        ObjectstoreCommands::Put(ObjectstorePutArgs {
            key,
            address,
            overwrite,
            input,
        }) => {
            let mut signer =
                get_signer(&provider, cli.wallet_pk.clone(), cli.chain_name.clone()).await?;
            let machine = ObjectStore::<HttpClient>::attach(*address);
            let mut reader = input.into_async_reader().await?;
            let mut first_chunk = vec![0; MAX_INTERNAL_OBJECT_LENGTH as usize];
            let upload_progress = ObjectProgressBar::new();

            match reader.read_exact(&mut first_chunk).await {
                Ok(first_chunk_size) => {
                    let (tx, rx) = mpsc::channel::<Vec<u8>>(1024);
                    let bytes_read = Arc::new(Mutex::new(0 as usize));
                    let cid = Arc::new(Mutex::new(Cid::new_v0(Code::Sha2_256.digest(&[]))?));

                    upload_progress.show_processing();

                    // Spawn the non-Send future within the local task set
                    // This is necessary because the clap's AsyncReader impl
                    // is not `Send`. LocalSet is used to spawn the non-Send futures.
                    let bytes_read_clone = bytes_read.clone();
                    let cid_clone = cid.clone();
                    let local_set = LocalSet::new();
                    let chunk = first_chunk[..first_chunk_size].to_vec();
                    local_set.spawn_local(async {
                        match process_read_chunk(reader, tx, chunk, bytes_read_clone, cid_clone)
                            .await
                        {
                            Ok(_) => {}
                            Err(e) => {
                                panic!("Error reading from input: {:?}", e);
                            }
                        }
                    });
                    local_set.await;

                    upload_progress.show_uploading();

                    let object_cid = cid.lock().await.clone();
                    let params = PutParams {
                        key: key.as_bytes().to_vec(),
                        kind: ObjectKind::External(object_cid),
                        overwrite: *overwrite,
                    };
                    let total_bytes = first_chunk_size as usize + *bytes_read.lock().await;
                    let response_cid = machine
                        .upload(
                            &mut signer,
                            object_client,
                            key.clone(),
                            object_cid,
                            rx,
                            total_bytes,
                            params.clone(),
                        )
                        .await?;

                    upload_progress.show_uploaded(response_cid.clone());
                    assert!(response_cid == object_cid);

                    upload_progress.show_cid_verified();
                    upload_progress.finish();

                    let tx = machine
                        .put(
                            &provider,
                            &mut signer,
                            params,
                            BroadcastMode::Commit,
                            Default::default(),
                        )
                        .await?;

                    print_json(&tx).unwrap();
                }
                Err(e) => {
                    // internal object
                    if e.kind() == io::ErrorKind::UnexpectedEof {
                        reader.read_to_end(&mut first_chunk).await?;

                        let tx = machine
                            .put(
                                &provider,
                                &mut signer,
                                PutParams {
                                    key: key.as_bytes().to_vec(),
                                    kind: ObjectKind::Internal(ByteBuf(first_chunk)),
                                    overwrite: *overwrite,
                                },
                                BroadcastMode::Commit,
                                Default::default(),
                            )
                            .await?;

                        print_json(&tx).unwrap();
                    } else {
                        return Err(e.into());
                    }
                }
            }
            Ok(())
        }
        ObjectstoreCommands::Get(args) => {
            // TODO: Handle range requests
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
                        if !resolved {
                            return Err(anyhow!("object is not resolved"));
                        }

                        let progress_bar = ObjectProgressBar::new();

                        // The `download` method is currently using /objectstore API
                        // since we have decided to keep the GET APIs intact for a while.
                        // If we decide to remove these APIs we can move to Object API
                        // for downloading the file with CID.
                        let file = File::create(args.output.clone()).await?;
                        machine
                            .download(object_client, key.to_string(), file)
                            .await?;

                        progress_bar.show_downloaded(cid, args.output.clone());
                        progress_bar.finish();

                        Ok(())
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

async fn process_read_chunk(
    mut reader: impl AsyncRead + Unpin,
    tx: Sender<Vec<u8>>,
    first_chunk: Vec<u8>,
    bytes_read: Arc<Mutex<usize>>,
    cid: Arc<Mutex<Cid>>,
) -> anyhow::Result<()> {
    // create a tmpfile to help with CID calculation
    let mut tmp = TempFile::new().await?;
    let mut upload_buffer = vec![0; 10 * 1024 * 1024];

    // write first chunk to temp file and also
    // to mpsc channel for uploading
    tx.send(first_chunk.clone()).await.unwrap();
    tmp.write_all(&first_chunk).await?;

    // read remaining bytes from the reader into temp file and mpsc channel
    loop {
        match reader.read(&mut upload_buffer).await {
            Ok(0) => {
                break;
            }
            Ok(n) => {
                if tx.send(upload_buffer[..n].to_vec()).await.is_err() {
                    return Err(anyhow!("error sending data to channel"))?;
                }
                let mut bytes = bytes_read.lock().await;
                *bytes += n;
                tmp.write_all(&upload_buffer[..n]).await?;
            }
            Err(e) => {
                return Err(e.into());
            }
        }
    }

    tmp.flush().await?;
    tmp.rewind().await?;
    let generated_cid = generate_cid(&mut tmp).await?;
    cid.lock().await.clone_from(&generated_cid);

    Ok(())
}

// === Progress Bar ===

struct ObjectProgressBar {
    inner: ProgressBar,
}

impl ObjectProgressBar {
    fn new() -> Self {
        let inner = ProgressBar::new_spinner();
        let tick_style = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
        let template = "{spinner:.green} [{elapsed_precise}] {msg}";
        inner.set_style(
            ProgressStyle::with_template(template)
                .unwrap()
                .tick_strings(tick_style),
        );
        inner.enable_steady_tick(std::time::Duration::from_millis(80));

        Self { inner }
    }

    fn show_processing(&self) {
        self.inner
            .println(format!("{}Processing object...", Emoji("🏗️  ", ""),));
    }

    fn show_uploading(&self) {
        self.inner
            .println(format!("{}Uploading object...", Emoji("📡  ", ""),));
    }

    fn show_uploaded(&self, cid: Cid) {
        self.inner
            .println(format!("{}Upload complete {}", Emoji("✔️  ", ""), cid));
    }

    fn show_downloaded(&self, cid: Cid, path: String) {
        self.inner.println(format!(
            "{}Downloaded object {} at {}",
            Emoji("✔️  ", ""),
            cid,
            path
        ));
    }

    fn show_cid_verified(&self) {
        self.inner
            .println(format!("{}CID verified...", Emoji("✅  ", ""),));
    }

    fn finish(&self) {
        self.inner.finish_and_clear();
    }
}
