//! Rutas de administración de la plataforma.
//!
//! Gated por `users.role = 'admin'`. Hoy nadie tiene admin por defecto;
//! para bootstrap manual:
//!
//!   UPDATE users SET role='admin' WHERE handle='tu_handle';
//!
//! Cuando exista flujo formal de invitar/elevar admins, esto cambia.

use axum::extract::{Query, State};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use serde::Deserialize;

use crate::audit::{self, AuditFilter};
use crate::auth::CurrentUser;
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::web::shared::current_year;
use crate::web::templates::{AdminAuditRow, AdminAuditTemplate};

pub fn router() -> Router<AppState> {
    Router::new().route("/admin/audit", get(audit_index))
}

#[derive(Debug, Deserialize, Default)]
struct AuditQuery {
    action_prefix: Option<String>,
    target_type: Option<String>,
    target_id: Option<String>,
    limit: Option<i64>,
}

async fn audit_index(
    State(state): State<AppState>,
    current: CurrentUser,
    Query(q): Query<AuditQuery>,
) -> AppResult<impl IntoResponse> {
    // Gate: solo platform admins ven el log entero. Owner de una company
    // que quiera ver actividad de su company debería ir a una vista más
    // estrecha — por ahora no la exponemos.
    if current.user.role != "admin" {
        return Err(AppError::Forbidden);
    }

    let filter = AuditFilter {
        action_prefix: q.action_prefix.as_deref().filter(|s| !s.is_empty()),
        target_type: q.target_type.as_deref().filter(|s| !s.is_empty()),
        target_id: q.target_id.as_deref().filter(|s| !s.is_empty()),
        actor_id: None,
        limit: q.limit.unwrap_or(100),
    };
    let rows = audit::list_recent(&state.db, filter.clone()).await?;

    let items = rows
        .into_iter()
        .map(|r| AdminAuditRow {
            id: r.id,
            at: r
                .created_at
                .format(&time::format_description::well_known::Rfc3339)
                .unwrap_or_default(),
            actor: r.actor_id.map(|u| u.to_string()).unwrap_or_default(),
            actor_ip: r.actor_ip.map(|i| i.ip().to_string()).unwrap_or_default(),
            action: r.action,
            target_type: r.target_type.unwrap_or_default(),
            target_id: r.target_id.unwrap_or_default(),
            metadata: r
                .metadata
                .map(|v| serde_json::to_string(&v).unwrap_or_default())
                .unwrap_or_default(),
        })
        .collect();

    Ok(AdminAuditTemplate {
        year: current_year(),
        handle: current.user.handle,
        account_role: current.user.role.clone(),
        rows: items,
        action_prefix: q.action_prefix.unwrap_or_default(),
        target_type: q.target_type.unwrap_or_default(),
        target_id: q.target_id.unwrap_or_default(),
    })
}
