use backend::{auth, build_app_router, config::AppConfig, db, storage, AppState};
use tracing::info;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = dotenvy::dotenv();

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("backend=info,tower_http=info")),
        )
        .init();

    let config = AppConfig::from_env();
    std::fs::create_dir_all(&config.data_dir)?;
    storage::ensure_upload_dirs(&config.uploads_dir)?;

    let pool = db::init_db(&config.database_url).await?;
    auth::ensure_initial_admin(
        &pool,
        &config.initial_admin_username,
        &config.initial_admin_password,
    )
    .await?;

    let listen_addr = config.listen_addr;
    let state = AppState::new(pool, config);
    let app = build_app_router(state);

    info!("Rental Pro backend listening on http://{listen_addr}");
    let listener = tokio::net::TcpListener::bind(listen_addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
