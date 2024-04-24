// Copyright 2024 ADM Contributors
// SPDX-License-Identifier: Apache-2.0, MIT

use anyhow::anyhow;
use cid::multihash::{Blake2bHasher, Code, Hasher, MultihashDigest};
use cid::Cid;
use clap::{Args, Subcommand};
use clap_stdin::FileOrStdin;
use console::{style, Emoji};
use fendermint_actor_machine::WriteAccess;
use fendermint_actor_objectstore::{GetParams, ListParams, Object, ObjectKind, PutParams};
use fendermint_vm_message::query::FvmQueryHeight;
use fvm_ipld_encoding::serde_bytes::ByteBuf;
use fvm_ipld_encoding::IPLD_RAW;
use fvm_shared::address::Address;
use indicatif::{ProgressBar, ProgressStyle};
use serde_json::json;
use std::sync::Arc;
use tendermint_rpc::HttpClient;
use tokio::io::AsyncRead;
use tokio::io::{self, AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc;
use tokio::sync::mpsc::Sender;
use tokio::sync::Mutex;
use tokio::task::LocalSet;

use adm_provider::{json_rpc::JsonRpcProvider, upload::ObjectClient, BroadcastMode};
use adm_sdk::machine::{objectstore::ObjectStore, Machine};

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

static CHECKMARK: Emoji<'_, '_> = Emoji("✅  ", "");
static BROADCAST: Emoji<'_, '_> = Emoji("📡  ", "");
static TRUCK: Emoji<'_, '_> = Emoji("🚚  ", "");
static FINISHED: Emoji<'_, '_> = Emoji("🎉  ", "");
static SPARKLE: Emoji<'_, '_> = Emoji("✨ ", ":-)");

pub async fn handle_objectstore(cli: Cli, args: &ObjectstoreArgs) -> anyhow::Result<()> {
    let provider = JsonRpcProvider::new_http(cli.rpc_url.clone(), None)?;

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
            let mut signer =
                get_signer(&provider, cli.wallet_pk.clone(), cli.chain_name.clone()).await?;
            let object_client = ObjectClient::new(cli.object_api_url, 1942764459484029);
            let machine = ObjectStore::<HttpClient>::attach(args.address);
            let key = args.key.as_str();
            let mut reader = args.input.into_async_reader().await?;

            let mut buf = vec![0; (MAX_INTERNAL_OBJECT_LENGTH + 1) as usize];

            // ========================================
            let bar = ProgressBar::new_spinner();
            bar.set_style(
                ProgressStyle::with_template("{spinner} {wide_msg}")
                    .unwrap()
                    .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]),
            );
            bar.println(format!(
                "{} {}Uploading object...",
                style("[1/2]").bold().dim(),
                TRUCK,
            ));
            bar.enable_steady_tick(std::time::Duration::from_millis(80));
            // ========================================

            match reader.read_exact(&mut buf).await {
                Ok(size) => {
                    let (tx, rx) = mpsc::channel::<Vec<u8>>(1024);
                    // Send the first chunk
                    tx.send(buf[..size].to_vec()).await.unwrap();

                    let hasher = ObjectHasher::new();
                    hasher.update(&buf[..size]).await;
                    let bytes_read = Arc::new(Mutex::new(size as usize));

                    // Spawn the non-Send future within the local task set
                    // This is necessary because the AsyncReader impl from clap
                    // is not `Send`
                    let hasher_clone = hasher.clone();
                    let bytes_read_clone = bytes_read.clone();
                    let local_set = LocalSet::new();
                    local_set.spawn_local(async {
                        match process_read_chunk(reader, tx, hasher_clone, bytes_read_clone).await {
                            Ok(_) => {}
                            Err(e) => {
                                panic!("Error reading from input: {:?}", e);
                            }
                        }
                    });
                    local_set.await;

                    let object_cid = hasher.cid().await;
                    let params = PutParams {
                        key: key.as_bytes().to_vec(),
                        kind: ObjectKind::External(object_cid),
                        overwrite: true, // TODO: make an arg
                    };
                    let response_cid = machine
                        .object_upload(
                            &mut signer,
                            object_client,
                            args.key.clone(),
                            object_cid,
                            rx,
                            *bytes_read.lock().await,
                            params.clone(),
                        )
                        .await?;

                    // ========================================
                    bar.println(format!(
                        "{} {}Uploaded object with CID: {}",
                        style("[1/2]").bold().dim(),
                        FINISHED,
                        response_cid
                    ));

                    // assert remote cid == cid
                    println!("response_cid: {}", response_cid);
                    println!("object_cid: {}", object_cid);
                    assert!(response_cid == object_cid);
                    bar.println(format!(
                        "{} {}Local CID matched with remote CID",
                        style("[1/2]").bold().dim(),
                        CHECKMARK,
                    ));

                    bar.finish_and_clear();

                    let bar2 = ProgressBar::new_spinner();
                    bar2.enable_steady_tick(std::time::Duration::from_millis(100));
                    bar2.set_style(
                        ProgressStyle::with_template("{prefix:.bold.dim} {spinner} {wide_msg}")
                            .unwrap()
                            .tick_chars("⠁⠂⠄⡀⢀⠠⠐⠈ "),
                    );
                    bar2.println(format!(
                        "{} {}Broadcasting transaction...",
                        style("[2/2]").bold().dim(),
                        BROADCAST,
                    ));
                    // ========================================
                    println!("CAlling put!!");
                    let tx = machine
                        .put(
                            &provider,
                            &mut signer,
                            params,
                            BroadcastMode::Commit,
                            Default::default(),
                        )
                        .await?;

                    // ========================================
                    bar2.println(format!(
                        "{} {}Transaction complete...",
                        style("[2/2]").bold().dim(),
                        SPARKLE,
                    ));

                    bar2.finish();
                    // ========================================

                    print_json(&tx).unwrap();

                    // TODO: Pipe input to ObjectAPI if greater than MAX_INTERNAL_OBJECT_SIZE
                    // TODO: Input size in unknown, we could read_exact up to limit, and if there's
                    // TODO: still more to read, rewind reader and upload to ObjectApi
                    // TODO: Below is a start on how we'd upload to ObjectApi if input is big enough
                    // TODO: This doesn't quite work with plain AsyncRead. All the examples use File, which must be Send.
                    // TODO: We might have to use tokio::sync::mpsc
                    // TODO: Here's a discussion about showing a progress bar: https://github.com/seanmonstar/reqwest/issues/879
                }
                Err(e) => {
                    // internal object
                    if e.kind() == io::ErrorKind::UnexpectedEof {
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

async fn process_read_chunk(
    mut reader: impl AsyncRead + Unpin,
    tx: Sender<Vec<u8>>,
    hasher: ObjectHasher,
    bytes_read: Arc<Mutex<usize>>,
) -> anyhow::Result<()> {
    let mut buffer = vec![0; 8 * 1024 * 1024];
    loop {
        match reader.read(&mut buffer).await {
            Ok(0) => return Ok(()),
            Ok(n) => {
                if tx.send(buffer[..n].to_vec()).await.is_err() {
                    return Err(anyhow!("error sending data to channel"))?;
                }
                hasher.update(&buffer[..n]).await;
                let mut total = bytes_read.lock().await;
                *total += n;
            }
            Err(e) => {
                return Err(e.into());
            }
        }
    }
}

#[derive(Clone)]
pub struct ObjectHasher {
    inner: Arc<Mutex<Blake2bHasher<32>>>,
}

impl ObjectHasher {
    pub fn new() -> Self {
        let hasher = Blake2bHasher::default();
        ObjectHasher {
            inner: Arc::new(Mutex::new(hasher)),
        }
    }
}

impl ObjectHasher {
    pub async fn update(&self, data: &[u8]) {
        self.inner.lock().await.update(data);
    }

    pub async fn finalize(&self) -> Vec<u8> {
        self.inner.lock().await.finalize().to_vec()
    }
}

impl ObjectHasher {
    pub async fn cid(&self) -> Cid {
        let digest = self.finalize().await;
        let digest = Code::Blake2b256.wrap(digest.as_slice()).unwrap();
        Cid::new_v1(IPLD_RAW, digest)
    }
}
