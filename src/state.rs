//! Estado compartido entre handlers.
//!
//! Solo cosas baratas de clonar (Pool, Key, Config). Si algo es costoso
//! envolverlo en Arc dentro de este struct, no fuera.

use axum::extract::FromRef;
use axum_extra::extract::cookie::Key;
use sqlx::PgPool;

use crate::config::Config;

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub cfg: Config,
    pub cookie_key: Key,
    pub pm_key: [u8; 32],
    pub email: crate::email::SharedEmailSender,
    pub storage: crate::storage::S3Storage,
}

impl AppState {
    pub fn new(
        db: PgPool,
        cfg: Config,
        cookie_key: Key,
        pm_key: [u8; 32],
        email: crate::email::SharedEmailSender,
        storage: crate::storage::S3Storage,
    ) -> Self {
        Self { db, cfg, cookie_key, pm_key, email, storage }
    }
}

/// Permite que `SignedCookieJar` extraiga la `Key` desde `AppState`.
impl FromRef<AppState> for Key {
    fn from_ref(state: &AppState) -> Key {
        state.cookie_key.clone()
    }
}
