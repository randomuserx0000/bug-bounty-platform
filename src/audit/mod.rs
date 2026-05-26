//! Audit log: registro append-only de cada acción sensible.
//!
//! La tabla `audit_log` ya existe en el schema. Este módulo expone un
//! helper `log()` y un catálogo de `action` strings. Los handlers llaman
//! `let _ = audit::log(...).await;` después de la mutación — si la
//! inserción falla, se loggea y la request continúa. Auditar nunca
//! debería romper la operación que pretende auditar.
//!
//! Convención del campo `action`: `<namespace>.<verb>`, ej. `user.login`,
//! `payout.mark_sent`. Las constantes están abajo.

use serde_json::Value as JsonValue;
use sqlx::types::ipnetwork::IpNetwork;
use sqlx::PgPool;
use std::net::IpAddr;
use uuid::Uuid;

#[derive(Debug, Clone, Default)]
pub struct AuditEntry<'a> {
    pub actor_id: Option<Uuid>,
    pub actor_ip: Option<IpAddr>,
    pub action: &'a str,
    pub target_type: Option<&'a str>,
    pub target_id: Option<String>,
    pub metadata: Option<JsonValue>,
}

impl<'a> AuditEntry<'a> {
    pub fn new(action: &'a str) -> Self {
        Self { action, ..Default::default() }
    }
    pub fn actor(mut self, id: Uuid) -> Self {
        self.actor_id = Some(id); self
    }
    pub fn ip(mut self, ip: IpAddr) -> Self {
        self.actor_ip = Some(ip); self
    }
    pub fn target(mut self, ttype: &'a str, tid: impl ToString) -> Self {
        self.target_type = Some(ttype);
        self.target_id = Some(tid.to_string());
        self
    }
    pub fn metadata(mut self, v: JsonValue) -> Self {
        self.metadata = Some(v); self
    }
}

/// Inserta una fila en `audit_log`. Best-effort: si falla, se loggea y
/// devuelve `Ok(())` igual para que el caller no tenga que envolver.
pub async fn log(pool: &PgPool, e: AuditEntry<'_>) {
    let ip_net = e.actor_ip.map(IpNetwork::from);
    let res = sqlx::query(
        "INSERT INTO audit_log (actor_id, actor_ip, action, target_type, target_id, metadata)
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(e.actor_id)
    .bind(ip_net)
    .bind(e.action)
    .bind(e.target_type)
    .bind(e.target_id.as_deref())
    .bind(e.metadata)
    .execute(pool)
    .await;
    if let Err(err) = res {
        tracing::error!(error = ?err, action = %e.action, "audit log insert failed");
    }
}

// ============================================================================
// Catálogo de acciones. Mantener sincronizado con los call sites.
// ============================================================================

// User / auth
pub const USER_SIGNUP: &str = "user.signup";
pub const USER_LOGIN: &str = "user.login";
pub const USER_LOGIN_FAILED: &str = "user.login_failed";
pub const USER_LOGOUT: &str = "user.logout";
pub const USER_UPDATE_HANDLE: &str = "user.update_handle";

// Companies / programs / assets
pub const COMPANY_CREATE: &str = "company.create";
pub const PROGRAM_CREATE: &str = "program.create";
pub const ASSET_CREATE: &str = "asset.create";
pub const ASSET_DELETE: &str = "asset.delete";

// Reports
pub const REPORT_CREATE: &str = "report.create";
pub const REPORT_STATE_CHANGE: &str = "report.state_change";
pub const REPORT_SEVERITY_CHANGE: &str = "report.severity_change";
pub const REPORT_BOUNTY_SET: &str = "report.bounty_set";
pub const REPORT_COMMENT_ADD: &str = "report.comment_add";

// Attachments
pub const ATTACHMENT_UPLOAD: &str = "attachment.upload";
pub const ATTACHMENT_DELETE: &str = "attachment.delete";

// Payment methods
pub const PM_CREATE: &str = "payment_method.create";
pub const PM_DELETE: &str = "payment_method.delete";
pub const PM_SET_DEFAULT: &str = "payment_method.set_default";

// Payouts / escrow (el flujo de dinero — los más críticos para auditar)
pub const PAYOUT_CREATED_PENDING: &str = "payout.created_pending";
pub const PAYOUT_CREATED_FAILED: &str = "payout.created_failed";
pub const PAYOUT_MARK_SENT: &str = "payout.mark_sent";
pub const PAYOUT_MARK_FAILED: &str = "payout.mark_failed";
pub const PAYOUT_RETRY: &str = "payout.retry";
pub const ESCROW_DEPOSIT: &str = "escrow.deposit";

// ============================================================================
// Lectura para UI de admin
// ============================================================================

#[derive(Debug, sqlx::FromRow)]
pub struct AuditRow {
    pub id: i64,
    pub actor_id: Option<Uuid>,
    pub actor_ip: Option<IpNetwork>,
    pub action: String,
    pub target_type: Option<String>,
    pub target_id: Option<String>,
    pub metadata: Option<JsonValue>,
    pub created_at: time::OffsetDateTime,
}

#[derive(Debug, Default, Clone)]
pub struct AuditFilter<'a> {
    pub action_prefix: Option<&'a str>,
    pub target_type: Option<&'a str>,
    pub target_id: Option<&'a str>,
    pub actor_id: Option<Uuid>,
    pub limit: i64,
}

pub async fn list_recent(
    pool: &PgPool,
    f: AuditFilter<'_>,
) -> Result<Vec<AuditRow>, sqlx::Error> {
    let limit = if f.limit <= 0 || f.limit > 500 { 100 } else { f.limit };
    sqlx::query_as::<_, AuditRow>(
        "SELECT id, actor_id, actor_ip, action, target_type, target_id, metadata, created_at
         FROM audit_log
         WHERE ($1::text IS NULL OR action LIKE $1 || '%')
           AND ($2::text IS NULL OR target_type = $2)
           AND ($3::text IS NULL OR target_id = $3)
           AND ($4::uuid IS NULL OR actor_id = $4)
         ORDER BY id DESC
         LIMIT $5",
    )
    .bind(f.action_prefix)
    .bind(f.target_type)
    .bind(f.target_id)
    .bind(f.actor_id)
    .bind(limit)
    .fetch_all(pool)
    .await
}
