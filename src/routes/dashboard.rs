//! Placeholder del dashboard. Solo existe para verificar el flujo de auth
//! end-to-end. Lo reemplazamos con contenido real más adelante.

use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;

use crate::auth::CurrentUser;
use crate::error::AppResult;
use crate::state::AppState;
use crate::web::templates::DashboardTemplate;

pub fn router() -> Router<AppState> {
    Router::new().route("/dashboard", get(index))
}

async fn index(current: CurrentUser) -> AppResult<impl IntoResponse> {
    Ok(DashboardTemplate {
        year: time::OffsetDateTime::now_utc().year(),
        handle: current.user.handle,
        role: current.user.role,
    })
}
