use log::{error, info, Level};
use serde_json::json;
use warp::log::Info;

/// Helper function to log details for each request at specific verbosity levels
/// ([Level::Info] or [Level::Error]).
pub fn log_request_details(request: Info) {
    let level = if request.status().as_u16() >= 500 {
        Level::Error
    } else {
        Level::Info
    };
    let addr = request
        .remote_addr()
        .unwrap_or_else(|| ([0, 0, 0, 0], 0).into());
    let duration = request.elapsed().as_millis();
    let log_data = json!({
        "method": request.method().as_str(),
        "path": request.path(),
        "status": request.status().as_u16(),
        "client_addr": addr,
        "duration_ms": duration
    });
    match level {
        Level::Error => error!("{}", log_data),
        Level::Info => info!("{}", log_data),
        // Only Error & Info are used (Trace, Debug, Warn also possible)
        _ => {}
    }
}

/// Helper function to log the incoming request body for a route when
/// [`Level::Info`] logging is enabled.
pub fn log_request_body(route: &str, body: &str) {
    let log_data = json!({
        "route": route,
        "body": body
    });
    info!("{}", log_data);
}
