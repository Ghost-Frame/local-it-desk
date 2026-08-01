//! Local IT Desk server process entry point.

use std::net::SocketAddr;

use local_it_desk_server::config::Config;
use tracing_subscriber::EnvFilter;

/// Migrates storage, binds the listener, and serves the application.
#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let config = Config::from_env();
    config
        .prepare_runtime_directories()
        .expect("failed to prepare runtime directories");
    let _runtime_lock =
        local_it_desk_server::runtime_lock::acquire_runtime_lock(&config.database_path)
            .expect("another Local IT Desk process is already using this database");

    let pool = local_it_desk_server::db::create_pool(&config.database_path);
    let connection = pool.get().await.expect("failed to get database connection");
    connection
        .interact(|connection| local_it_desk_server::db::migrations::run_migrations(connection))
        .await
        .expect("migration interaction failed")
        .expect("migration failed");

    let listen_addr = config.listen_addr;
    let app = local_it_desk_server::routes::build_router(config, pool);
    let listener = tokio::net::TcpListener::bind(listen_addr)
        .await
        .expect("failed to bind listener");
    tracing::info!(address = %listen_addr, "Local IT Desk listening");
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .expect("server failed");
}
