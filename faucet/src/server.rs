use log::info;
use warp::Filter;

use crate::Cli;

use util::log_request_details;

mod register;
mod send;
mod shared;
mod util;

/// Server entrypoint for the faucet service.
pub async fn run(cli: Cli) -> anyhow::Result<()> {
    let faucet_pk = cli.private_key;
    let listen_addr = cli.listen;
    let token_address = cli.token_address;

    let register_route = register::register_route(faucet_pk.clone());
    let send_route = send::send_route(faucet_pk.clone(), token_address);

    let log_request_details = warp::log::custom(log_request_details);

    let router = register_route
        .or(send_route)
        .with(
            warp::cors()
                .allow_any_origin()
                .allow_headers(vec!["Content-Type"])
                .allow_methods(vec!["POST"]),
        )
        .with(log_request_details)
        .recover(shared::handle_rejection);

    info!("Starting server at {}", listen_addr);
    warp::serve(router).run(listen_addr).await;
    Ok(())
}
