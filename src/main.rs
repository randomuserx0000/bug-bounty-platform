//! bugbounty-platform: punto de entrada.
//!
//! Solo carga config, abre la DB, arma el router y escucha. Toda la lógica
//! vive en submódulos; este archivo debe mantenerse trivial.

use std::net::SocketAddr;

mod audit;
mod auth;
mod config;
mod db;
mod domain;
mod email;
mod error;
mod payments;
mod routes;
mod state;
mod storage;
mod web;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Carga .env si existe (dev). En prod las vars vienen del entorno real.
    let _ = dotenvy::dotenv();

    // Logging estructurado. RUST_LOG controla el nivel.
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,bugbounty=debug,sqlx=warn".into()),
        )
        .with_target(true)
        .compact()
        .init();

    let cfg = config::Config::from_env()?;
    tracing::info!(addr = %cfg.bind_addr, "starting bugbounty-platform");

    let pool = db::connect(&cfg).await?;
    db::migrate(&pool).await?;

    let cookie_key = cfg.cookie_key()?;
    // Falla temprano si la key de payment_methods está mal: si arranca,
    // arranca seguro.
    let pm_key = payments::crypto::key_from_hex(&cfg.payment_methods_key_hex)?;
    let email_sender: email::SharedEmailSender = std::sync::Arc::new(
        email::SmtpSender::new(&cfg.smtp_host, cfg.smtp_port, cfg.smtp_from.clone()),
    );
    // Object storage (MinIO en dev). Falla temprano si el bucket no responde.
    let object_store = storage::S3Storage::from_config(&cfg).await?;
    tracing::info!(bucket = %cfg.s3_bucket, "object storage ready");

    let app_state =
        state::AppState::new(pool, cfg.clone(), cookie_key, pm_key, email_sender, object_store);
    let app = routes::router(app_state);

    let addr: SocketAddr = cfg.bind_addr.parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(%addr, "listening");
    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

/// Espera Ctrl+C o SIGTERM para cerrar limpio. Importante para no perder
/// requests en vuelo cuando el orquestador reinicia el contenedor.
async fn shutdown_signal() {
    use tokio::signal;
    let ctrl_c = async {
        let _ = signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut s) = signal::unix::signal(signal::unix::SignalKind::terminate()) {
            s.recv().await;
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    tracing::info!("shutdown signal received");
}
