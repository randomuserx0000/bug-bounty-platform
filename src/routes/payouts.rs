//! Handlers de payouts.
//!
//! - `/manage/:company_slug/payouts` — cola de la company (owners/admins).
//! - `/payouts/mine` — vista del researcher.
//! - `POST /payouts/:id/mark-sent` — admin pega tx_ref. Pasa a `sent`.
//! - `POST /payouts/:id/mark-failed` — admin marca inviable. Reembolsa escrow.
//! - `POST /payouts/:id/retry` — re-encolar un payout failed (usa default actual del reporter).
//! - `POST /manage/:company_slug/escrow/deposit` — admin ajusta saldo manual
//!   (en prod: lo dispara un evento de deposito on-chain o transferencia).

use axum::extract::{Path, State};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Form, Router};
use serde::Deserialize;
use uuid::Uuid;

use crate::audit;
use crate::auth::CurrentUser;
use crate::db;
use crate::domain::ids::{PayoutId, UserId};
use crate::domain::payout::PayoutStatus;
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::web::shared::{current_year, error_fragment, htmx_redirect_owned};
use crate::web::templates::{
    EscrowDepositForm, MinePayoutsTemplate, PayoutQueueRow, PayoutsQueueTemplate,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/manage/:company_slug/payouts", get(queue))
        .route(
            "/manage/:company_slug/escrow/deposit",
            post(escrow_deposit),
        )
        .route("/payouts/mine", get(mine))
        .route("/payouts/:id/mark-sent", post(mark_sent))
        .route("/payouts/:id/mark-failed", post(mark_failed))
        .route("/payouts/:id/retry", post(retry))
}

// ----------------------------------------------------------------------------
// Admin queue
// ----------------------------------------------------------------------------

async fn queue(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(company_slug): Path<String>,
) -> AppResult<impl IntoResponse> {
    let company = db::companies::find_by_slug(&state.db, &company_slug)
        .await?
        .ok_or(AppError::NotFound)?;
    let m = db::companies::membership(&state.db, company.id, UserId::from(current.user.id))
        .await?
        .ok_or(AppError::Forbidden)?;
    if !m.role.can_manage_programs() {
        return Err(AppError::Forbidden);
    }

    let rows = db::payouts::list_for_company(&state.db, company.id).await?;
    // Para mostrar el handle del reporter al lado del payout, leemos en batch.
    let mut items = Vec::with_capacity(rows.len());
    for p in rows {
        let reporter = db::users::find_by_id(&state.db, p.user_id.0).await.ok().flatten();
        let report = db::reports::find_by_id(&state.db, p.report_id).await.ok().flatten();
        items.push(PayoutQueueRow {
            id: p.id.to_string(),
            report_public_id: report.map(|r| r.public_id).unwrap_or_default(),
            reporter_handle: reporter.map(|u| u.handle).unwrap_or_default(),
            rail: p.rail.display_name().into(),
            amount_usd: format!("${:.2}", p.amount_cents as f64 / 100.0),
            status: p.status.as_str().into(),
            tx_ref: p.tx_ref.unwrap_or_default(),
            error_message: p.error_message.unwrap_or_default(),
        });
    }

    let escrow = db::companies::escrow_balance(&state.db, company.id).await?;
    Ok(PayoutsQueueTemplate {
        year: current_year(),
        handle: current.user.handle,
        account_role: current.user.role.clone(),
        company_slug: company.slug,
        company_name: company.display_name,
        escrow_usd: format!("${:.2}", escrow as f64 / 100.0),
        payouts: items,
    })
}

// ----------------------------------------------------------------------------
// Researcher view
// ----------------------------------------------------------------------------

async fn mine(
    State(state): State<AppState>,
    current: CurrentUser,
) -> AppResult<impl IntoResponse> {
    let rows = db::payouts::list_for_reporter(&state.db, UserId::from(current.user.id)).await?;
    let items = rows
        .into_iter()
        .map(|p| PayoutQueueRow {
            id: p.id.to_string(),
            report_public_id: String::new(), // se rellena abajo
            reporter_handle: String::new(),
            rail: p.rail.display_name().into(),
            amount_usd: format!("${:.2}", p.amount_cents as f64 / 100.0),
            status: p.status.as_str().into(),
            tx_ref: p.tx_ref.unwrap_or_default(),
            error_message: p.error_message.unwrap_or_default(),
        })
        .collect();
    // (En una iteración futura: traer public_id del report con un JOIN. Por
    // ahora la mine view es simple — el researcher ya conoce sus reports.)
    Ok(MinePayoutsTemplate {
        year: current_year(),
        handle: current.user.handle,
        account_role: current.user.role.clone(),
        payouts: items,
    })
}

// ----------------------------------------------------------------------------
// Mutaciones
// ----------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct MarkSentForm {
    tx_ref: String,
}

async fn mark_sent(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(id): Path<Uuid>,
    Form(form): Form<MarkSentForm>,
) -> AppResult<Response> {
    let payout = require_admin_for_payout(&state, &current, PayoutId(id)).await?;
    if form.tx_ref.trim().is_empty() {
        return Ok(error_fragment("tx_ref requerido"));
    }
    if !matches!(payout.status, PayoutStatus::Pending | PayoutStatus::Processing) {
        return Ok(error_fragment("solo se puede marcar enviado un payout pending/processing"));
    }

    db::payouts::mark_sent(&state.db, payout.id, form.tx_ref.trim()).await?;
    audit::log(&state.db, audit::AuditEntry::new(audit::PAYOUT_MARK_SENT)
        .actor(current.user.id).target("payout", payout.id)
        .metadata(serde_json::json!({
            "amount_cents": payout.amount_cents,
            "rail": payout.rail.as_str(),
            "tx_ref": form.tx_ref.trim(),
        }))).await;

    // Notificar al reporter.
    if let Ok(Some(user)) = db::users::find_by_id(&state.db, payout.user_id.0).await {
        let _ = state.email.send(&crate::email::Email {
            to: user.email,
            subject: format!("bounty enviado · USD {:.2}", payout.amount_cents as f64 / 100.0),
            text_body: format!("Tu bounty fue enviado. Referencia: {}", form.tx_ref.trim()),
            html_body: String::new(),
        }).await;
    }

    let company = db::companies::find_by_id(&state.db, payout.company_id)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(htmx_redirect_owned(format!("/manage/{}/payouts", company.slug)))
}

#[derive(Debug, Deserialize)]
struct MarkFailedForm {
    reason: String,
}

async fn mark_failed(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(id): Path<Uuid>,
    Form(form): Form<MarkFailedForm>,
) -> AppResult<Response> {
    let payout = require_admin_for_payout(&state, &current, PayoutId(id)).await?;
    let reason = form.reason.trim();
    if reason.is_empty() {
        return Ok(error_fragment("explica la razón del fallo"));
    }
    db::payouts::mark_failed_and_refund(&state.db, payout.id, reason).await?;
    audit::log(&state.db, audit::AuditEntry::new(audit::PAYOUT_MARK_FAILED)
        .actor(current.user.id).target("payout", payout.id)
        .metadata(serde_json::json!({
            "amount_cents": payout.amount_cents,
            "refunded": true,
            "reason": reason,
        }))).await;
    let company = db::companies::find_by_id(&state.db, payout.company_id)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(htmx_redirect_owned(format!("/manage/{}/payouts", company.slug)))
}

async fn retry(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(id): Path<Uuid>,
) -> AppResult<Response> {
    let payout = require_admin_for_payout(&state, &current, PayoutId(id)).await?;
    if !matches!(payout.status, PayoutStatus::Failed) {
        return Ok(error_fragment("solo se reintentan payouts en failed"));
    }
    let pm = db::payment_methods::find_default_for_user(&state.db, payout.user_id)
        .await?
        .ok_or_else(|| AppError::Validation("el reporter sigue sin método de pago".into()))?;

    match db::payouts::retry_failed(&state.db, payout.id, pm.id, pm.rail).await {
        Ok(()) => {}
        Err(e) if e.to_string().contains("escrow_balance_cents") => {
            return Ok(error_fragment("escrow insuficiente; deposita primero"));
        }
        Err(e) => return Err(e.into()),
    }
    audit::log(&state.db, audit::AuditEntry::new(audit::PAYOUT_RETRY)
        .actor(current.user.id).target("payout", payout.id)
        .metadata(serde_json::json!({
            "amount_cents": payout.amount_cents,
            "new_payment_method": pm.id.to_string(),
            "new_rail": pm.rail.as_str(),
        }))).await;
    let company = db::companies::find_by_id(&state.db, payout.company_id)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(htmx_redirect_owned(format!("/manage/{}/payouts", company.slug)))
}

// ----------------------------------------------------------------------------
// Escrow deposit (admin manual)
// ----------------------------------------------------------------------------

async fn escrow_deposit(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(company_slug): Path<String>,
    Form(form): Form<EscrowDepositForm>,
) -> AppResult<Response> {
    let company = db::companies::find_by_slug(&state.db, &company_slug)
        .await?
        .ok_or(AppError::NotFound)?;
    let m = db::companies::membership(&state.db, company.id, UserId::from(current.user.id))
        .await?
        .ok_or(AppError::Forbidden)?;
    if !m.role.can_manage_programs() {
        return Err(AppError::Forbidden);
    }
    let usd: i64 = form
        .amount_usd
        .trim()
        .parse()
        .map_err(|_| AppError::Validation("monto inválido".into()))?;
    if usd <= 0 {
        return Ok(error_fragment("monto debe ser > 0"));
    }
    let cents = usd
        .checked_mul(100)
        .ok_or_else(|| AppError::Validation("overflow".into()))?;
    db::companies::adjust_escrow(&state.db, company.id, cents).await?;
    audit::log(&state.db, audit::AuditEntry::new(audit::ESCROW_DEPOSIT)
        .actor(current.user.id).target("company", company.id)
        .metadata(serde_json::json!({ "amount_cents": cents }))).await;
    Ok(htmx_redirect_owned(format!("/manage/{}/payouts", company.slug)))
}

// ----------------------------------------------------------------------------
// helpers
// ----------------------------------------------------------------------------

async fn require_admin_for_payout(
    state: &AppState,
    current: &CurrentUser,
    id: PayoutId,
) -> AppResult<crate::domain::payout::PayoutRecord> {
    let payout = db::payouts::find_by_id(&state.db, id)
        .await?
        .ok_or(AppError::NotFound)?;
    let m = db::companies::membership(&state.db, payout.company_id, UserId::from(current.user.id))
        .await?
        .ok_or(AppError::Forbidden)?;
    if !m.role.can_manage_programs() {
        return Err(AppError::Forbidden);
    }
    Ok(payout)
}
