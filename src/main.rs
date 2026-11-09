use std::{net::SocketAddr, sync::Arc, time::Duration};

use anyhow::{Context, Result};
use axum_boilerplate::{
    auth::{google::GoogleJwksVerifier, jwt::JwtKeys},
    config::Config,
    cors,
    rate_limit::RateLimit,
    repositories::{
        SeaOrmDeviceRepository, SeaOrmItemRepository, SeaOrmOtpRepository, SeaOrmRbacRepository, SeaOrmTokenBlacklist,
        SeaOrmUserRepository, TokenBlacklist,
    },
    routes, seed,
    services::{auth::OtpSettings, storage::LocalFileStore},
    state::{AppState, Deps, Settings},
};
use sea_orm::{ConnectOptions, Database};
use tokio::{net::TcpListener, signal};
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv_override().ok();
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info,tower_http=debug".into()))
        .with(fmt::layer())
        .init();

    let config = Config::from_env()?;
    tracing::info!(?config, "loaded configuration");

    let mut opts = ConnectOptions::new(config.database_url.clone());
    opts.max_connections(50).min_connections(5).connect_timeout(Duration::from_secs(8)).sqlx_logging(false);
    let db = Database::connect(opts).await.context("connecting to database")?;

    use migration::MigratorTrait;
    migration::Migrator::up(&db, None).await.context("running migrations")?;

    seed::run(&db, &config.admin).await.context("seeding")?;

    let jwt = Arc::new(JwtKeys::new(&config.jwt)?);
    let rl = &config.rate_limit;
    let auth_limiter = RateLimit::per_minute(rl.auth_per_minute, rl.trust_proxy_headers);
    let general_limiter = RateLimit::per_minute(rl.general_per_minute, rl.trust_proxy_headers);
    auth_limiter.spawn_cleanup(Duration::from_secs(300));
    general_limiter.spawn_cleanup(Duration::from_secs(300));

    let blacklist = Arc::new(SeaOrmTokenBlacklist::new(db.clone()));
    spawn_blacklist_purge(blacklist.clone());

    let mut state = AppState::new(
        Deps {
            users: Arc::new(SeaOrmUserRepository::new(db.clone())),
            blacklist,
            otp: Arc::new(SeaOrmOtpRepository::new(db.clone())),
            devices: Arc::new(SeaOrmDeviceRepository::new(db.clone())),
            rbac: Arc::new(SeaOrmRbacRepository::new(db.clone())),
            items: Arc::new(SeaOrmItemRepository::new(db.clone())),
            files: Arc::new(LocalFileStore::new(&config.upload_dir)),
        },
        Settings {
            otp: OtpSettings {
                expiry_minutes: config.otp_expiry_minutes,
                expose_otp: config.environment == "development",
            },
            staff_email_domains: config.allowed_staff_email_domains.clone(),
            max_upload_bytes: config.max_upload_bytes,
            upload_dir: config.upload_dir.clone(),
            admin_email: config.admin.email.clone(),
            frontend_base_url: config.frontend_base_url.clone(),
        },
        jwt,
        auth_limiter,
        general_limiter,
    );
    if let Some(id) = &config.google_client_id {
        state = state.with_google(Arc::new(GoogleJwksVerifier::new(id.clone())));
    }
    let app = routes::router_with_docs(state, cors::layer(&config.cors_origins)?, config.docs_enabled);

    let listener = TcpListener::bind((config.host.as_str(), config.port)).await?;
    tracing::info!("listening on {}", listener.local_addr()?);
    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    tracing::info!("shutdown complete");
    Ok(())
}

fn spawn_blacklist_purge(blacklist: Arc<dyn TokenBlacklist>) {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(3600));
        loop {
            tick.tick().await;
            match blacklist.purge_expired().await {
                Ok(n) if n > 0 => tracing::info!(removed = n, "purged expired blacklist entries"),
                Ok(_) => {}
                Err(e) => tracing::warn!(error = %e, "blacklist purge failed"),
            }
        }
    });
}

async fn shutdown_signal() {
    let ctrl_c = async { signal::ctrl_c().await.expect("ctrl-c handler") };
    #[cfg(unix)]
    let term = async {
        signal::unix::signal(signal::unix::SignalKind::terminate()).expect("SIGTERM handler").recv().await;
    };
    #[cfg(not(unix))]
    let term = std::future::pending::<()>();
    tokio::select! { _ = ctrl_c => {}, _ = term => {} }
}
