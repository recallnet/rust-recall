use crate::machine::{objectstore::ObjectStore, DeployTx, Machine};
use crate::TxArgs;
use adm_provider::{json_rpc::JsonRpcProvider, object::ObjectClient, BroadcastMode, Tx};
use adm_signer::{key::read_secret_key, AccountKind, Signer, Wallet};
use anyhow::anyhow;
use cid::Cid;
pub use fendermint_actor_machine::WriteAccess;
pub use fendermint_actor_objectstore::{
    DeleteParams, GetParams, ListParams, Object, ObjectKind, PutParams,
};
use fendermint_actor_objectstore::{ObjectList, ObjectListItem};
use fendermint_vm_core::chainid;
use fendermint_vm_message::query::FvmQueryHeight;
pub use fvm_ipld_encoding::serde_bytes::ByteBuf;
use std::str::FromStr;
use tendermint_rpc::{HttpClient, Url};

pub async fn get_signer(
    provider: &JsonRpcProvider,
    pk: &str,
    chain_name: &str,
) -> anyhow::Result<Wallet> {
    let chain_id = chainid::from_str_hashed(&chain_name)?;
    let sk = read_secret_key(&pk)?;
    let mut wallet = Wallet::new_secp256k1(sk, AccountKind::Ethereum, chain_id)?;
    wallet.init_sequence(provider).await?;

    Ok(wallet)
}

pub fn get_provider(rpc_url: &str, proxy_url: Option<&str>) -> JsonRpcProvider {
    let rpc_url = Url::from_str(rpc_url).unwrap();
    let proxy_url = proxy_url.map(|url| Url::from_str(url).unwrap());

    JsonRpcProvider::new_http(rpc_url, proxy_url).unwrap()
}

pub fn get_chain_id(chain_name: &str) -> u64 {
    let chain_name = String::from(chain_name);
    let chain_id = chainid::from_str_hashed(&chain_name).unwrap();

    u64::from(chain_id)
}

pub fn get_object_client(object_api_url: &str, chain_id: u64) -> ObjectClient {
    let object_api_url = Url::from_str(object_api_url).unwrap();

    ObjectClient::new(object_api_url, chain_id)
}

pub async fn create_object_store(
    provider: &JsonRpcProvider,
    signer: &mut impl Signer,
    access_type: Option<WriteAccess>,
    tx_args: Option<TxArgs>,
) -> anyhow::Result<(ObjectStore<HttpClient>, DeployTx)> {
    let access_type = access_type.unwrap_or(WriteAccess::OnlyOwner);
    let tx_args = tx_args.unwrap_or_default();
    let (store, tx) = ObjectStore::new(provider, signer, access_type, tx_args).await?;
    Ok((store, tx))
}

pub async fn put_object(
    provider: &JsonRpcProvider,
    signer: &mut impl Signer,
    store: &ObjectStore<HttpClient>,
    params: PutParams,
    broadcast_mode: Option<BroadcastMode>,
    tx_args: Option<TxArgs>,
) -> anyhow::Result<Tx<Cid>> {
    let tx_args = tx_args.unwrap_or_default();
    let broadcast_mode = broadcast_mode.unwrap_or(BroadcastMode::Commit);
    let tx = store
        .put(provider, signer, params, broadcast_mode, tx_args)
        .await?;

    Ok(tx)
}

// For `internal` only
pub fn to_put_params(key: &str, value: &str, overwrite: Option<bool>) -> PutParams {
    let key = key.as_bytes().to_vec();
    let value = value.as_bytes().to_vec();
    let kind = ObjectKind::Internal(ByteBuf(value));
    let overwrite = overwrite.unwrap_or(false);

    PutParams {
        key,
        kind,
        overwrite,
    }
}

pub async fn get_object(
    provider: &JsonRpcProvider,
    store: &ObjectStore<HttpClient>,
    key: &str,
    height: Option<FvmQueryHeight>,
) -> anyhow::Result<Option<Object>> {
    let key = key.as_bytes().to_vec();
    let params = GetParams { key };
    let height = height.unwrap_or(FvmQueryHeight::Committed);
    let obj = store.get(provider, params, height).await?;

    Ok(obj)
}

pub async fn list_objects(
    provider: &JsonRpcProvider,
    store: &ObjectStore<HttpClient>,
    mut params: Option<ListParams>,
    height: Option<FvmQueryHeight>,
) -> anyhow::Result<Option<ObjectList>> {
    let effective_params = params.take().unwrap_or_else(|| ListParams {
        prefix: Vec::new(),
        delimiter: '/'.to_string().as_bytes().to_vec(),
        offset: 0,
        limit: 0,
    });
    let effective_height = height.unwrap_or(FvmQueryHeight::Committed);
    let objs = store
        .list(provider, effective_params, effective_height)
        .await?;

    Ok(objs)
}

pub async fn delete_object(
    provider: &JsonRpcProvider,
    signer: &mut impl Signer,
    store: &ObjectStore<HttpClient>,
    key: &str,
    broadcast_mode: Option<BroadcastMode>,
    tx_args: Option<TxArgs>,
) -> anyhow::Result<Tx<Cid>> {
    let key = key.as_bytes().to_vec();
    let tx_args = tx_args.unwrap_or_default();
    let broadcast_mode = broadcast_mode.unwrap_or(BroadcastMode::Commit);
    let params = DeleteParams { key };
    let tx = store
        .delete(provider, signer, params, broadcast_mode, tx_args)
        .await?;

    Ok(tx)
}

fn object_to_string(obj: Object) -> anyhow::Result<String> {
    match obj {
        Object::Internal(byte_buf) => String::from_utf8(byte_buf.into_vec())
            .map_err(|e| anyhow!("failed to decode bytes into a string: {}", e)),
        Object::External((byte_buf, _flag)) => {
            // Ignore the external / resolved bool for now and assume resolved
            String::from_utf8(byte_buf.into_vec())
                .map_err(|e| anyhow!("failed to decode bytes into a string: {}", e))
        }
    }
}

pub fn parse_object(obj: Option<Object>) -> Option<String> {
    match obj {
        Some(obj) => match object_to_string(obj) {
            Ok(string) => Some(string),
            Err(_e) => None,
        },
        None => None,
    }
}

fn object_list_to_string(item: ObjectListItem) -> anyhow::Result<String> {
    match item {
        // Note: should just use serde_json::json
        ObjectListItem::Internal((cid, size)) => {
            let json_string = format!(
                "{{\"content\":\"{}\",\"kind\":\"internal\",\"size\":{}}}",
                cid.to_string(),
                size
            );
            Ok(json_string)
        }
        ObjectListItem::External((cid, resolved)) => Ok(format!(
            "{{\"content\":\"{}\",\"kind\":\"external\",\"resolved\":{}}}",
            cid.to_string(),
            resolved
        )),
    }
}

pub fn parse_object_list(object_list: Option<ObjectList>) -> anyhow::Result<Vec<String>> {
    let list = object_list.map_or(Ok(Vec::new()), |list| {
        list.objects
            .into_iter()
            .map(|(key, item)| {
                let key_str =
                    String::from_utf8(key).map_err(|e| anyhow!("failed to decode key: {}", e));
                let value_str = object_list_to_string(item);
                key_str.and_then(|k| value_str.map(|v| format!("{{\"{}\":\"{}\"}}", k, v)))
            })
            .collect()
    });

    list
}
