//! Conexión a Postgres, migraciones, y queries por entidad.
//!
//! Cada entidad tiene su propio submódulo con queries puras (sin lógica
//! de negocio). Los handlers componen estas queries; no llaman a `sqlx`
//! directamente.

use secrecy::ExposeSecret;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use std::time::Duration;

use crate::config::Config;

pub mod assets;
pub mod attachments;
pub mod email_verifications;
pub mod companies;
pub mod courses;
pub mod dashboard;
pub mod osint;
pub mod payment_methods;
pub mod payouts;
pub mod programs;
pub mod report_events;
pub mod reports;
pub mod sessions;
pub mod users;

pub async fn connect(cfg: &Config) -> anyhow::Result<PgPool> {
    let pool = PgPoolOptions::new()
        .max_connections(cfg.db_max_conn)
        .acquire_timeout(Duration::from_secs(5))
        .connect(cfg.database_url.expose_secret())
        .await?;
    Ok(pool)
}

pub async fn migrate(pool: &PgPool) -> anyhow::Result<()> {
    sqlx::migrate!("./migrations").run(pool).await?;
    tracing::info!("migrations applied");
    Ok(())
}
