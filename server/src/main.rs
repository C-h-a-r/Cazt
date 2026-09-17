use std::net::SocketAddr;

use cazt::config::Config;
use cazt::logging;
use cazt::state::AppState;
use tracing::info;

#[tokio::main]
async fn main() {
    let config = Config::from_env();
    logging::init(&config);

    let state = AppState::new(config.clone());
    tokio::spawn(cazt::discovery::run_loop(
        state.devices.clone(),
        config.clone(),
    ));
    tokio::spawn(cazt::api::status_loop(state.clone()));

    let addr = config.listen_addr();
    info!(%addr, "Cazt listening");
    if let Some(host) = cazt::net::advertised_host(&config) {
        info!(%host, "Advertising LAN host for media proxy");
    } else {
        tracing::warn!("No LAN address detected. Set CAZT_PUBLIC_HOST if TVs cannot reach Cazt.");
    }

    let app = cazt::api::router(state);
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .unwrap_or_else(|err| panic!("failed to bind {addr}: {err}"));
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .expect("server error");
}
