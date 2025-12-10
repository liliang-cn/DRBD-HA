use std::net::SocketAddr;
use std::sync::Arc;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use drbd_ha::{api, config::AppConfig, state::AppState};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 1. Load configuration first
    let config = AppConfig::load();

    // 2. Setup EnvFilter
    // Use config level if RUST_LOG is not set
    let log_level_str = std::env::var("RUST_LOG")
        .unwrap_or_else(|_| format!("drbd_ha={0},tower_http={0}", config.log.level));
    let env_filter = tracing_subscriber::EnvFilter::new(log_level_str);

    // 3. Setup Layers
    let stdout_layer = tracing_subscriber::fmt::layer()
        .with_writer(std::io::stdout)
        .with_ansi(true);

    // We need to keep the guard alive for the duration of the program
    let mut _file_guard = None;

    let file_layer = if let Some(path_str) = &config.log.file {
        let path = std::path::Path::new(path_str);
        let directory = path.parent().unwrap_or(std::path::Path::new("."));
        let filename = path
            .file_name()
            .unwrap_or(std::ffi::OsStr::new("drbd-ha.log"));

        let file_appender = tracing_appender::rolling::daily(directory, filename);
        let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);
        _file_guard = Some(guard);

        Some(
            tracing_subscriber::fmt::layer()
                .with_writer(non_blocking)
                .with_ansi(false),
        )
    } else {
        None
    };

    // 4. Init Subscriber
    tracing_subscriber::registry()
        .with(env_filter)
        .with(stdout_layer)
        .with(file_layer)
        .init();

    tracing::info!("Logging initialized");
    if let Some(f) = &config.log.file {
        tracing::info!("Log output enabled to file: {}", f);
    }
    tracing::info!("Configuration loaded");

    // Initialize application state
    // Use with_local_node to ensure DB is opened and local node exists
    let state = Arc::new(AppState::with_local_node(config.clone()).await?);

    // Start background tasks
    // Currently no background tasks are implemented on AppState
    // let sync_state = state.clone();
    // tokio::spawn(async move {
    //     // Placeholder for background tasks
    // });

    // Create router
    let app = api::router::create_router(state.clone());

    // Start server
    let addr = SocketAddr::from(([0, 0, 0, 0], config.server.port));
    tracing::info!("listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
