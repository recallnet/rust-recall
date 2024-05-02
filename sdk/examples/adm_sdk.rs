extern crate adm_provider;
extern crate adm_sdk;
extern crate adm_signer;

use crate::adm_sdk::machine::Machine;
// use adm_provider::object::ObjectService;
use adm_sdk::helpers::{
    create_object_store, delete_object,
    /* get_chain_id, */ get_object, /* get_object_client, */
    get_provider, get_signer, list_objects, parse_object, parse_object_list, put_object,
    to_put_params,
};
// use reqwest::Body;
// use std::fs::File;
// use std::io::Read;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Set up signer and chain connection
    let chain_name = "test";
    let wallet_pk = "1c323d494d1d069fe4c891350a1ec691c4216c17418a0cb3c7533b143bd2b812";
    let rpc_url = "http://127.0.0.1:26657";
    let provider = get_provider(rpc_url, None);
    let mut signer = get_signer(&provider, wallet_pk, chain_name).await?;

    // Create object store
    let (os, deploy_tx) = create_object_store(&provider, &mut signer, None, None).await?;
    println!(
        "Object store '{}' created at tx hash '{}'",
        os.address().to_string(),
        deploy_tx.hash
    );

    // Put an object into the store directly onchain
    let key = "hello";
    let val = "world";
    let params = to_put_params(key, val, None);
    let mut tx = put_object(&provider, &mut signer, &os, params, None, None).await?;
    println!(
        "Onchain object put at tx hash '{}' with key '{}'",
        tx.hash, key
    );

    // List all objects and get the object just created
    let mut list = list_objects(&provider, &os, None, None).await?;
    let mut objs = parse_object_list(list).unwrap();
    println!(
        "Current objects in store '{}': {:?}",
        os.address().to_string(),
        objs
    );
    println!("Getting object at key '{}'...", key);
    let obj = get_object(&provider, &os, key, None).await?;
    let result = parse_object(obj).unwrap();
    println!("Value at key '{}': '{}'", key, result);

    // Delete the object
    tx = delete_object(&provider, &mut signer, &os, key, None, None).await?;
    println!("Object at key '{}' deleted at tx hash '{}'", key, tx.hash);
    list = list_objects(&provider, &os, None, None).await?;
    objs = parse_object_list(list).unwrap();
    println!(
        "Current objects in store '{}': {:?}",
        os.address().to_string(),
        objs
    );

    // // TODO: the provider `upload` method doesn't point to the correct
    // // endpoint yet, and this code is likely incorrect
    // // Connect to an object store HTTP client and put a file object into the
    // // store via the API
    // let chain_id = get_chain_id(&chain_name);
    // let object_api_url = "http://127.0.0.1:8001";
    // let os_client = get_object_client(object_api_url, chain_id);
    // let key = "file";
    // let path = std::env::temp_dir().join("file.txt");
    // std::fs::write(&path, "hello world").unwrap();
    // let mut file = File::open(&path).map_err(anyhow::Error::new)?;
    // let mut buffer = Vec::new();
    // file.read_to_end(&mut buffer).map_err(anyhow::Error::new)?;
    // let body = Body::from(buffer);
    // let file_len = std::fs::metadata(&path).map_err(anyhow::Error::new)?.len() as usize;
    // let msg = "File upload via API".to_string();
    // let upload_tx = os_client.upload(body, file_len, msg).await?;
    // println!("File object put at CID '{}'", upload_tx.cid.to_string(),);
    // std::fs::remove_file(&path)
    //     .map(|_| ())
    //     .map_err(anyhow::Error::new)?;

    Ok(())
}
