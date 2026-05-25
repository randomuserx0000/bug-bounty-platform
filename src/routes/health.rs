//! Endpoints de salud para el orquestador (Docker/k8s/systemd).
//!
//! - /healthz: el proceso está vivo. No toca DB.
//! - /readyz:  estamos listos para recibir tráfico (DB reachable).

use axum::extract::State;
use axum::http::StatusCode;

use crate::state::AppState;

pub async fn healthz() -> &'static str {
    "ok"
}

pub async fn readyz(State(state): State<AppState>) -> (StatusCode, &'static str) {
    match sqlx::query_scalar::<_, i32>("SELECT 1").fetch_one(&state.db).await {
        Ok(_)  => (StatusCode::OK, "ready"),
        Err(e) => {
            tracing::warn!(error = ?e, "readyz: db not reachable");
            (StatusCode::SERVICE_UNAVAILABLE, "not ready")
        }
    }
}
