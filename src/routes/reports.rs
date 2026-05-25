//! Handlers de reports.
//!
//! - **Researcher** (cualquier user logueado): puede crear un report sobre
//!   un programa público, ver SUS reports, comentar en ellos, retirar
//!   (rejected) o pedir/responder needs_info.
//! - **Triager** (miembro de la company con can_manage_programs): ve los
//!   reports del programa, puede comentar (internal+external), cambiar
//!   estado/severity/bounty.
//!
//! Para acceder a un report, el usuario debe ser reporter O member de la
//! company del programa. Cualquier otro → 404 (no filtramos existencia).

use askama::Template;
use axum::extract::{DefaultBodyLimit, Multipart, Path, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Form, Router};
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::audit;
use crate::auth::CurrentUser;
use crate::db;
use crate::db::attachments::NewAttachment;
use crate::db::reports::NewReport;
use crate::domain::ids::{AssetId, AttachmentId, UserId};
use crate::domain::program::{ProgramStatus, ProgramVisibility};
use crate::domain::report::{
    can_actor_transition, ActorKind, EventType, ReportSeverity, ReportState,
};
use crate::email::Email;
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::web::shared::{current_year, error_fragment, htmx_redirect_owned};
use crate::web::templates::{
    AttachmentView, EventView, MyReportRow, ReportFormTemplate, ReportListTemplate,
    ReportShowTemplate, TriageListTemplate,
};

/// Tamaño máximo por upload (suma de todos los campos del multipart).
/// 50 MiB es cómodo para PoCs típicos sin destrozar memoria si llegan
/// varios uploads concurrentes.
const ATTACHMENT_MAX_BYTES: usize = 50 * 1024 * 1024;

pub fn router() -> Router<AppState> {
    Router::new()
        // Researcher: crear desde la página pública del programa
        .route(
            "/programs/:company_slug/:program_slug/reports/new",
            get(new_form),
        )
        .route(
            "/programs/:company_slug/:program_slug/reports",
            post(create),
        )
        // Mis reports (researcher view)
        .route("/reports", get(my_reports))
        // Triage list (company view)
        .route(
            "/manage/:company_slug/programs/:program_slug/reports",
            get(triage_list),
        )
        // Detalle + acciones (mismo URL para ambos lados)
        .route("/reports/:public_id", get(show))
        .route("/reports/:public_id/comments", post(add_comment))
        .route("/reports/:public_id/state", post(change_state))
        .route("/reports/:public_id/severity", post(change_severity))
        .route("/reports/:public_id/bounty", post(set_bounty))
        // Attachments: el upload necesita un body limit propio (50 MB).
        .route(
            "/reports/:public_id/attachments",
            post(upload_attachment)
                .layer(DefaultBodyLimit::max(ATTACHMENT_MAX_BYTES)),
        )
        .route(
            "/reports/:public_id/attachments/:id",
            get(download_attachment),
        )
        .route(
            "/reports/:public_id/attachments/:id/delete",
            post(delete_attachment),
        )
}

// ----------------------------------------------------------------------------
// CREATE FORM + POST
// ----------------------------------------------------------------------------

async fn new_form(
    State(state): State<AppState>,
    current: CurrentUser,
    Path((company_slug, program_slug)): Path<(String, String)>,
) -> AppResult<impl IntoResponse> {
    let (company, program) = require_public_program(&state, &company_slug, &program_slug).await?;
    let assets = db::assets::list_for_program(&state.db, program.id).await?;
    let asset_options = assets
        .into_iter()
        .filter(|a| a.in_scope)
        .map(|a| (a.id.to_string(), format!("[{}] {}", a.asset_type.as_str(), a.label)))
        .collect();
    Ok(ReportFormTemplate {
        year: current_year(),
        handle: current.user.handle,
        company_slug: company.slug,
        company_name: company.display_name,
        program_slug: program.slug,
        program_name: program.name,
        assets: asset_options,
    })
}

#[derive(Debug, Deserialize)]
struct CreateForm {
    title: String,
    asset_id: Option<String>,
    description_md: String,
    impact_md: Option<String>,
    repro_md: Option<String>,
    cwe: Option<String>,
    cvss_vector: Option<String>,
    severity: Option<String>,
}

async fn create(
    State(state): State<AppState>,
    current: CurrentUser,
    Path((company_slug, program_slug)): Path<(String, String)>,
    Form(form): Form<CreateForm>,
) -> AppResult<Response> {
    let (_company, program) = require_public_program(&state, &company_slug, &program_slug).await?;

    if form.title.trim().is_empty() {
        return Ok(error_fragment("título requerido"));
    }
    if form.description_md.trim().len() < 30 {
        return Ok(error_fragment("la descripción debe tener al menos 30 caracteres"));
    }

    let asset_id = match form.asset_id.as_deref() {
        None | Some("") => None,
        Some(s) => match uuid::Uuid::parse_str(s) {
            Ok(u) => Some(AssetId(u)),
            Err(_) => return Ok(error_fragment("asset inválido")),
        },
    };

    let severity = form
        .severity
        .as_deref()
        .and_then(ReportSeverity::from_str)
        .unwrap_or(ReportSeverity::None);

    let (report_id, public_id) = db::reports::create(
        &state.db,
        NewReport {
            program_id: program.id,
            asset_id,
            reporter_id: UserId::from(current.user.id),
            title: form.title.trim(),
            description_md: form.description_md.trim(),
            impact_md: opt(&form.impact_md),
            repro_md: opt(&form.repro_md),
            cwe: opt(&form.cwe),
            cvss_vector: opt(&form.cvss_vector),
            severity,
        },
    )
    .await?;

    audit::log(&state.db, audit::AuditEntry::new(audit::REPORT_CREATE)
        .actor(current.user.id).target("report", report_id)
        .metadata(serde_json::json!({
            "public_id": public_id,
            "program_id": program.id.to_string(),
            "severity": severity.as_str(),
        }))).await;

    // Notificación (LogOnly por ahora): a los owners de la company.
    let _ = send_company_notification(
        &state,
        program.company_id,
        &format!("[{public_id}] nuevo report: {}", form.title.trim()),
        &format!(
            "Un nuevo report fue enviado al programa {}.\n\n{}",
            program.name,
            form.description_md.trim().chars().take(400).collect::<String>()
        ),
    )
    .await;

    Ok(htmx_redirect_owned(format!("/reports/{public_id}")))
}

// ----------------------------------------------------------------------------
// LISTS
// ----------------------------------------------------------------------------

async fn my_reports(
    State(state): State<AppState>,
    current: CurrentUser,
) -> AppResult<impl IntoResponse> {
    let rows = db::reports::list_for_reporter(&state.db, UserId::from(current.user.id)).await?;
    let items = rows.into_iter().map(report_row_view).collect();
    Ok(ReportListTemplate {
        year: current_year(),
        handle: current.user.handle,
        reports: items,
    })
}

async fn triage_list(
    State(state): State<AppState>,
    current: CurrentUser,
    Path((company_slug, program_slug)): Path<(String, String)>,
) -> AppResult<impl IntoResponse> {
    let (company, program) = require_member(&state, &current, &company_slug, &program_slug).await?;
    let rows = db::reports::list_for_program(&state.db, program.id).await?;
    let items = rows.into_iter().map(report_row_view).collect();
    Ok(TriageListTemplate {
        year: current_year(),
        handle: current.user.handle,
        company_slug: company.slug,
        program_slug: program.slug,
        program_name: program.name,
        reports: items,
    })
}

fn report_row_view(r: crate::domain::report::ReportRecord) -> MyReportRow {
    MyReportRow {
        public_id: r.public_id,
        title: r.title,
        state: r.state.as_str().into(),
        severity: r.severity.as_str().into(),
    }
}

// ----------------------------------------------------------------------------
// SHOW
// ----------------------------------------------------------------------------

async fn show(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(public_id): Path<String>,
) -> AppResult<impl IntoResponse> {
    let ctx = load_report_ctx(&state, &current, &public_id).await?;

    let events = db::report_events::list_for_report(&state.db, ctx.report.id, ctx.is_triager)
        .await?;
    let events_view = events
        .into_iter()
        .map(|e| EventView {
            event_type: e.event_type.clone(),
            body_html: e.body_md.as_deref().map(render_markdown).unwrap_or_default(),
            metadata_text: e
                .metadata
                .as_ref()
                .map(|v| v.to_string())
                .unwrap_or_default(),
            is_internal: e.is_internal,
            at: e.created_at.format(&time::format_description::well_known::Rfc3339)
                .unwrap_or_default(),
        })
        .collect();

    // Attachments asociados al report.
    let attachments_db = db::attachments::list_for_report(&state.db, ctx.report.id).await?;
    let attachments_view = attachments_db
        .into_iter()
        .map(|a| AttachmentView {
            id: a.id.to_string(),
            filename: a.filename,
            mime: a.mime,
            size_human: human_size(a.size_bytes),
            kind: a.kind,
            sha256_short: hex::encode(&a.sha256[..a.sha256.len().min(6)]),
            can_delete: ctx.is_triager || a.uploader_id == UserId::from(current.user.id),
        })
        .collect();

    let r = &ctx.report;
    let actor = if ctx.is_triager { ActorKind::Triager } else { ActorKind::Reporter };
    let next_states = ReportState::from_str(r.state.as_str())
        .map(|cur| {
            all_states()
                .into_iter()
                .filter(|s| can_actor_transition(actor, cur, *s))
                .map(|s| s.as_str().to_string())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    Ok(ReportShowTemplate {
        year: current_year(),
        handle: current.user.handle,
        public_id: r.public_id.clone(),
        title: r.title.clone(),
        state: r.state.as_str().into(),
        severity: r.severity.as_str().into(),
        description_html: render_markdown(&r.description_md),
        impact_html: r.impact_md.as_deref().map(render_markdown).unwrap_or_default(),
        repro_html: r.repro_md.as_deref().map(render_markdown).unwrap_or_default(),
        cwe: r.cwe.clone().unwrap_or_default(),
        cvss_vector: r.cvss_vector.clone().unwrap_or_default(),
        bounty_usd: r.bounty_amount_cents.map(|c| format!("${}", c / 100)).unwrap_or_default(),
        is_triager: ctx.is_triager,
        is_reporter: ctx.is_reporter,
        next_states,
        events: events_view,
        attachments: attachments_view,
    })
}

// ----------------------------------------------------------------------------
// COMMENTS / STATE / SEVERITY / BOUNTY
// ----------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct CommentForm {
    body_md: String,
    internal: Option<String>,
}

async fn add_comment(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(public_id): Path<String>,
    Form(form): Form<CommentForm>,
) -> AppResult<Response> {
    let ctx = load_report_ctx(&state, &current, &public_id).await?;
    if form.body_md.trim().is_empty() {
        return Ok(error_fragment("el comentario no puede estar vacío"));
    }
    // Solo triagers pueden marcar internal.
    let is_internal = ctx.is_triager && form.internal.as_deref() == Some("on");

    db::report_events::create(
        &state.db,
        db::report_events::NewEvent {
            report_id: ctx.report.id,
            actor_id: Some(UserId::from(current.user.id)),
            event_type: EventType::Comment,
            body_md: Some(form.body_md.trim()),
            metadata: None,
            is_internal,
        },
    )
    .await?;
    audit::log(&state.db, audit::AuditEntry::new(audit::REPORT_COMMENT_ADD)
        .actor(current.user.id).target("report", ctx.report.id)
        .metadata(serde_json::json!({
            "public_id": ctx.report.public_id, "is_internal": is_internal,
        }))).await;

    // Notificación al "otro lado" si el comment es externo.
    if !is_internal {
        let _ = notify_other_side(&state, &ctx, &current, &public_id, "nuevo comentario").await;
    }

    Ok(htmx_redirect_owned(format!("/reports/{public_id}")))
}

#[derive(Debug, Deserialize)]
struct StateForm {
    target: String,
    comment: Option<String>,
}

async fn change_state(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(public_id): Path<String>,
    Form(form): Form<StateForm>,
) -> AppResult<Response> {
    let ctx = load_report_ctx(&state, &current, &public_id).await?;
    let target = ReportState::from_str(&form.target)
        .ok_or_else(|| AppError::Validation("estado desconocido".into()))?;

    let actor = if ctx.is_triager { ActorKind::Triager } else { ActorKind::Reporter };
    if !can_actor_transition(actor, ctx.report.state, target) {
        return Ok(error_fragment("transición no permitida"));
    }

    db::reports::update_state(&state.db, ctx.report.id, target).await?;
    audit::log(&state.db, audit::AuditEntry::new(audit::REPORT_STATE_CHANGE)
        .actor(current.user.id).target("report", ctx.report.id)
        .metadata(serde_json::json!({
            "public_id": ctx.report.public_id,
            "from": ctx.report.state.as_str(),
            "to": target.as_str(),
        }))).await;
    db::report_events::create(
        &state.db,
        db::report_events::NewEvent {
            report_id: ctx.report.id,
            actor_id: Some(UserId::from(current.user.id)),
            event_type: EventType::StateChange,
            body_md: opt(&form.comment),
            metadata: Some(json!({ "from": ctx.report.state.as_str(), "to": target.as_str() })),
            is_internal: false,
        },
    )
    .await?;

    let _ = notify_other_side(
        &state, &ctx, &current, &public_id,
        &format!("estado: {} → {}", ctx.report.state.as_str(), target.as_str()),
    ).await;

    // Hook de payouts: si pasamos a `resolved` y hay bounty fijado, generar
    // el payout (pending o failed según método de pago + escrow). Errores
    // se loggean — no queremos que un problema de payout bloquee el cambio
    // de estado, que ya quedó persistido.
    if matches!(target, ReportState::Resolved) {
        if let Some(amount) = ctx.report.bounty_amount_cents {
            if amount > 0 {
                match maybe_create_payout(&state, &ctx, amount as i64).await {
                    Ok(()) => {}
                    Err(e) => tracing::error!(error = ?e, "create_payout failed"),
                }
            }
        }
    }

    Ok(htmx_redirect_owned(format!("/reports/{public_id}")))
}

/// Decide qué payout crear cuando un report cierra con bounty:
/// - si reporter NO tiene método de pago → `failed` con error claro + email
/// - si reporter tiene → intenta `pending` con débito de escrow
///   - si escrow insuficiente → `failed` con error + email
///   - si todo OK → email al reporter "bounty aprobado, en cola"
async fn maybe_create_payout(
    state: &AppState,
    ctx: &ReportCtx,
    amount_cents: i64,
) -> anyhow::Result<()> {
    use crate::db::payouts::NewPayoutInput;

    let report = &ctx.report;

    // Necesitamos company_id; lo resolvemos del program.
    let program = db::programs::find_by_id(&state.db, report.program_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("program desapareció"))?;

    let pm = db::payment_methods::find_default_for_user(&state.db, report.reporter_id).await?;

    // CASO 1: sin método de pago → failed.
    let Some(pm) = pm else {
        let input = NewPayoutInput {
            report_id: report.id,
            company_id: program.company_id,
            user_id: report.reporter_id,
            payment_method_id: None,
            rail: crate::payments::PaymentRail::UsdtTrc20, // placeholder; el rail es NOT NULL en SQL
            amount_cents,
        };
        let pid = db::payouts::create_failed(
            &state.db,
            input,
            "reporter sin método de pago configurado",
        ).await?;
        audit::log(&state.db, audit::AuditEntry::new(audit::PAYOUT_CREATED_FAILED)
            .target("payout", pid)
            .metadata(serde_json::json!({
                "report_id": report.id.to_string(),
                "public_id": report.public_id,
                "amount_cents": amount_cents,
                "reason": "no_payment_method",
            }))).await;
        let email = crate::email::Email {
            to: ctx.reporter_email.clone(),
            subject: format!("[{}] bounty aprobado, falta método de pago", report.public_id),
            text_body: format!(
                "Tu report fue resuelto con bounty USD {:.2}, pero no tienes método de pago \
                 configurado. Añade uno en {}/settings/payment-methods y luego pediremos \
                 reintento del pago.",
                amount_cents as f64 / 100.0,
                state.cfg.public_url
            ),
            html_body: String::new(),
        };
        let _ = state.email.send(&email).await;
        return Ok(());
    };

    let input = NewPayoutInput {
        report_id: report.id,
        company_id: program.company_id,
        user_id: report.reporter_id,
        payment_method_id: Some(pm.id),
        rail: pm.rail,
        amount_cents,
    };

    // CASO 2: intentar débito + pending. Si escrow rompe el CHECK,
    // caemos a failed con error explícito.
    match db::payouts::create_pending_with_debit(&state.db, input).await {
        Ok(payout_id) => {
            audit::log(&state.db, audit::AuditEntry::new(audit::PAYOUT_CREATED_PENDING)
                .target("payout", payout_id)
                .metadata(serde_json::json!({
                    "report_id": report.id.to_string(),
                    "public_id": report.public_id,
                    "amount_cents": amount_cents,
                    "rail": pm.rail.as_str(),
                    "payment_method_id": pm.id.to_string(),
                }))).await;
            let email = crate::email::Email {
                to: ctx.reporter_email.clone(),
                subject: format!("[{}] bounty aprobado: USD {:.2}", report.public_id, amount_cents as f64 / 100.0),
                text_body: format!(
                    "Tu bounty fue aprobado y está en cola para pago al método {} ({}).",
                    pm.rail.display_name(),
                    pm.label.as_deref().unwrap_or("default")
                ),
                html_body: String::new(),
            };
            let _ = state.email.send(&email).await;
            Ok(())
        }
        Err(e) => {
            // CHECK violation por escrow negativo es el caso esperado.
            let msg = if e.to_string().contains("escrow_balance_cents") {
                "escrow insuficiente de la company"
            } else {
                "error creando payout"
            };
            // Crear con el mismo método de pago pero status=failed; no toca escrow.
            let fallback = NewPayoutInput {
                report_id: report.id,
                company_id: program.company_id,
                user_id: report.reporter_id,
                payment_method_id: Some(pm.id),
                rail: pm.rail,
                amount_cents,
            };
            let pid = db::payouts::create_failed(&state.db, fallback, msg).await?;
            audit::log(&state.db, audit::AuditEntry::new(audit::PAYOUT_CREATED_FAILED)
                .target("payout", pid)
                .metadata(serde_json::json!({
                    "report_id": report.id.to_string(),
                    "amount_cents": amount_cents,
                    "reason": msg,
                }))).await;
            let _ = state.email.send(&crate::email::Email {
                to: ctx.reporter_email.clone(),
                subject: format!("[{}] bounty pendiente: {}", report.public_id, msg),
                text_body: format!(
                    "El bounty quedó marcado como pendiente por: {msg}. La company debe reponer \
                     escrow para que el pago se reintente."
                ),
                html_body: String::new(),
            }).await;
            // Notificar también a owners de la company.
            let _ = send_company_notification(
                state,
                program.company_id,
                &format!("[{}] payout failed: {}", report.public_id, msg),
                &format!("Bounty USD {:.2}. Razón: {}", amount_cents as f64 / 100.0, msg),
            ).await;
            Ok(())
        }
    }
}

#[derive(Debug, Deserialize)]
struct SeverityForm {
    severity: String,
}

async fn change_severity(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(public_id): Path<String>,
    Form(form): Form<SeverityForm>,
) -> AppResult<Response> {
    let ctx = load_report_ctx(&state, &current, &public_id).await?;
    if !ctx.is_triager {
        return Err(AppError::Forbidden);
    }
    let severity = ReportSeverity::from_str(&form.severity)
        .ok_or_else(|| AppError::Validation("severity desconocida".into()))?;

    db::reports::update_severity(&state.db, ctx.report.id, severity).await?;
    audit::log(&state.db, audit::AuditEntry::new(audit::REPORT_SEVERITY_CHANGE)
        .actor(current.user.id).target("report", ctx.report.id)
        .metadata(serde_json::json!({
            "public_id": ctx.report.public_id,
            "from": ctx.report.severity.as_str(),
            "to": severity.as_str(),
        }))).await;
    db::report_events::create(
        &state.db,
        db::report_events::NewEvent {
            report_id: ctx.report.id,
            actor_id: Some(UserId::from(current.user.id)),
            event_type: EventType::SeverityChange,
            body_md: None,
            metadata: Some(json!({
                "from": ctx.report.severity.as_str(),
                "to": severity.as_str(),
            })),
            is_internal: false,
        },
    )
    .await?;

    Ok(htmx_redirect_owned(format!("/reports/{public_id}")))
}

#[derive(Debug, Deserialize)]
struct BountyForm {
    amount_usd: String,
}

async fn set_bounty(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(public_id): Path<String>,
    Form(form): Form<BountyForm>,
) -> AppResult<Response> {
    let ctx = load_report_ctx(&state, &current, &public_id).await?;
    if !ctx.is_triager {
        return Err(AppError::Forbidden);
    }
    let usd: i32 = form
        .amount_usd
        .trim()
        .parse()
        .map_err(|_| AppError::Validation("monto inválido".into()))?;
    if usd < 0 {
        return Ok(error_fragment("monto debe ser >= 0"));
    }
    let cents = usd.checked_mul(100).ok_or_else(|| AppError::Validation("overflow".into()))?;

    db::reports::update_bounty(&state.db, ctx.report.id, cents).await?;
    audit::log(&state.db, audit::AuditEntry::new(audit::REPORT_BOUNTY_SET)
        .actor(current.user.id).target("report", ctx.report.id)
        .metadata(serde_json::json!({
            "public_id": ctx.report.public_id,
            "amount_cents": cents,
            "prev_amount_cents": ctx.report.bounty_amount_cents,
        }))).await;
    db::report_events::create(
        &state.db,
        db::report_events::NewEvent {
            report_id: ctx.report.id,
            actor_id: Some(UserId::from(current.user.id)),
            event_type: EventType::BountySet,
            body_md: None,
            metadata: Some(json!({ "amount_cents": cents })),
            is_internal: false,
        },
    )
    .await?;

    let _ = notify_other_side(
        &state, &ctx, &current, &public_id,
        &format!("bounty fijado en ${usd}"),
    ).await;

    Ok(htmx_redirect_owned(format!("/reports/{public_id}")))
}

// ----------------------------------------------------------------------------
// CONTEXT + AUTH HELPERS
// ----------------------------------------------------------------------------

struct ReportCtx {
    report: crate::domain::report::ReportRecord,
    is_reporter: bool,
    is_triager: bool,
    /// Email del reporter para notificaciones.
    reporter_email: String,
}

async fn load_report_ctx(
    state: &AppState,
    current: &CurrentUser,
    public_id: &str,
) -> AppResult<ReportCtx> {
    let report = db::reports::find_by_public_id(&state.db, public_id)
        .await?
        .ok_or(AppError::NotFound)?;

    let user_id = UserId::from(current.user.id);
    let is_reporter = report.reporter_id == user_id;

    let program = db::programs::find_by_id(&state.db, report.program_id)
        .await?
        .ok_or(AppError::NotFound)?;
    let m = db::companies::membership(&state.db, program.company_id, user_id).await?;
    let is_triager = m
        .map(|mb| mb.role.can_manage_programs())
        .unwrap_or(false);

    if !is_reporter && !is_triager {
        return Err(AppError::NotFound);
    }

    let reporter_email = db::users::find_by_id(&state.db, report.reporter_id.0)
        .await?
        .map(|u| u.email)
        .unwrap_or_default();

    Ok(ReportCtx { report, is_reporter, is_triager, reporter_email })
}

async fn require_public_program(
    state: &AppState,
    company_slug: &str,
    program_slug: &str,
) -> AppResult<(crate::domain::company::CompanyRecord, crate::domain::program::ProgramRecord)> {
    let company = db::companies::find_by_slug(&state.db, company_slug)
        .await?
        .ok_or(AppError::NotFound)?;
    let program = db::programs::find_by_company_and_slug(&state.db, company.id, program_slug)
        .await?
        .ok_or(AppError::NotFound)?;
    let is_public = matches!(program.visibility, ProgramVisibility::Public)
        && matches!(program.status, ProgramStatus::Public);
    if !is_public {
        return Err(AppError::NotFound);
    }
    Ok((company, program))
}

async fn require_member(
    state: &AppState,
    current: &CurrentUser,
    company_slug: &str,
    program_slug: &str,
) -> AppResult<(crate::domain::company::CompanyRecord, crate::domain::program::ProgramRecord)> {
    let company = db::companies::find_by_slug(&state.db, company_slug)
        .await?
        .ok_or(AppError::NotFound)?;
    let m = db::companies::membership(&state.db, company.id, UserId::from(current.user.id))
        .await?
        .ok_or(AppError::Forbidden)?;
    if !m.role.can_manage_programs() {
        return Err(AppError::Forbidden);
    }
    let program = db::programs::find_by_company_and_slug(&state.db, company.id, program_slug)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok((company, program))
}

// ----------------------------------------------------------------------------
// helpers
// ----------------------------------------------------------------------------

fn opt(s: &Option<String>) -> Option<&str> {
    s.as_deref().map(str::trim).filter(|x| !x.is_empty())
}

// ----------------------------------------------------------------------------
// ATTACHMENTS
// ----------------------------------------------------------------------------

/// Upload multipart: lee el archivo + un campo `kind` opcional, calcula
/// sha256 server-side, sube a MinIO con storage_key único, inserta row.
async fn upload_attachment(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(public_id): Path<String>,
    mut multipart: Multipart,
) -> AppResult<Response> {
    let ctx = load_report_ctx(&state, &current, &public_id).await?;

    let mut file_bytes: Option<bytes::Bytes> = None;
    let mut filename = String::new();
    let mut content_type = String::new();
    let mut kind = String::from("other");

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::Validation(format!("multipart: {e}")))?
    {
        let name = field.name().unwrap_or_default().to_string();
        match name.as_str() {
            "file" => {
                filename = field.file_name().unwrap_or("upload").to_string();
                content_type = field.content_type().unwrap_or("application/octet-stream").to_string();
                let data = field.bytes().await
                    .map_err(|e| AppError::Validation(format!("read upload: {e}")))?;
                file_bytes = Some(data);
            }
            "kind" => {
                let v = field.text().await.unwrap_or_default();
                if matches!(v.as_str(),
                    "poc" | "firmware" | "pcap" | "schematic" | "sdr_iq" | "video" | "other"
                ) {
                    kind = v;
                }
            }
            _ => { let _ = field.bytes().await; } // descartar campos desconocidos
        }
    }

    let Some(data) = file_bytes else {
        return Ok(error_fragment("falta el archivo"));
    };
    if data.is_empty() {
        return Ok(error_fragment("archivo vacío"));
    }
    let size_bytes = i64::try_from(data.len()).unwrap_or(i64::MAX);

    // Sanitizar el filename: solo basename, sin path separators.
    let safe_name = sanitize_filename(&filename);
    if safe_name.is_empty() {
        return Ok(error_fragment("nombre de archivo inválido"));
    }

    // MIME pragmático: si el cliente envió uno razonable lo usamos; si no,
    // adivinamos por extensión.
    let mime = if content_type == "application/octet-stream" || content_type.is_empty() {
        mime_guess::from_path(&safe_name)
            .first_or_octet_stream()
            .essence_str()
            .to_string()
    } else {
        content_type
    };

    // sha256 server-side.
    let mut hasher = Sha256::new();
    hasher.update(&data);
    let sha = hasher.finalize().to_vec();

    // storage_key único: reports/{public_id}/{uuid}-{filename}.
    let storage_key = format!(
        "reports/{public_id}/{}-{safe_name}",
        Uuid::new_v4()
    );

    state
        .storage
        .put(&storage_key, data, &mime)
        .await
        .map_err(|e| anyhow::anyhow!("storage put: {e}"))?;

    let id = db::attachments::create(
        &state.db,
        NewAttachment {
            report_id: ctx.report.id,
            uploader_id: UserId::from(current.user.id),
            filename: &safe_name,
            mime: &mime,
            size_bytes,
            sha256: &sha,
            storage_key: &storage_key,
            kind: &kind,
        },
    )
    .await?;
    audit::log(&state.db, audit::AuditEntry::new(audit::ATTACHMENT_UPLOAD)
        .actor(current.user.id).target("attachment", id)
        .metadata(serde_json::json!({
            "report_id": ctx.report.id.to_string(),
            "public_id": ctx.report.public_id,
            "filename": safe_name,
            "size_bytes": size_bytes,
            "kind": kind,
            "sha256_hex": hex::encode(&sha),
        }))).await;

    // Evento en la timeline para que ambos lados vean el upload.
    let _ = db::report_events::create(
        &state.db,
        db::report_events::NewEvent {
            report_id: ctx.report.id,
            actor_id: Some(UserId::from(current.user.id)),
            event_type: EventType::System,
            body_md: Some(&format!("adjuntó **{safe_name}** ({})", human_size(size_bytes))),
            metadata: Some(json!({ "attachment_id": id.0, "kind": kind })),
            is_internal: false,
        },
    )
    .await;

    Ok(htmx_redirect_owned(format!("/reports/{public_id}")))
}

async fn download_attachment(
    State(state): State<AppState>,
    current: CurrentUser,
    Path((public_id, id)): Path<(String, Uuid)>,
) -> AppResult<Response> {
    let ctx = load_report_ctx(&state, &current, &public_id).await?;
    let att = db::attachments::find_by_id(&state.db, AttachmentId(id))
        .await?
        .ok_or(AppError::NotFound)?;
    // El attachment debe pertenecer al report cuyo permiso ya verificamos.
    if att.report_id != ctx.report.id {
        return Err(AppError::NotFound);
    }

    let (bytes_data, _ct) = state
        .storage
        .get_bytes(&att.storage_key)
        .await
        .map_err(|e| match e {
            crate::storage::StorageError::NotFound => AppError::NotFound,
            other => anyhow::anyhow!("storage get: {other}").into(),
        })?;

    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(&att.mime).unwrap_or(HeaderValue::from_static("application/octet-stream")),
    );
    // attachment; filename=... fuerza descarga + protege contra renderizar
    // HTML/SVG malicioso del researcher en el contexto del dominio.
    let disp = format!("attachment; filename=\"{}\"", att.filename.replace('"', ""));
    if let Ok(v) = HeaderValue::from_str(&disp) {
        headers.insert(header::CONTENT_DISPOSITION, v);
    }

    Ok((StatusCode::OK, headers, bytes_data).into_response())
}

async fn delete_attachment(
    State(state): State<AppState>,
    current: CurrentUser,
    Path((public_id, id)): Path<(String, Uuid)>,
) -> AppResult<Response> {
    let ctx = load_report_ctx(&state, &current, &public_id).await?;
    let att = db::attachments::find_by_id(&state.db, AttachmentId(id))
        .await?
        .ok_or(AppError::NotFound)?;
    if att.report_id != ctx.report.id {
        return Err(AppError::NotFound);
    }
    // Solo el uploader o un triager pueden borrar.
    let user_id = UserId::from(current.user.id);
    if att.uploader_id != user_id && !ctx.is_triager {
        return Err(AppError::Forbidden);
    }

    // Borrar primero del object store; si falla la DB, queda objeto huérfano
    // pero no fila apuntando a key inexistente (que es peor).
    let _ = state.storage.delete(&att.storage_key).await;
    db::attachments::delete(&state.db, AttachmentId(id), ctx.report.id).await?;
    audit::log(&state.db, audit::AuditEntry::new(audit::ATTACHMENT_DELETE)
        .actor(current.user.id).target("attachment", id)
        .metadata(serde_json::json!({
            "report_id": ctx.report.id.to_string(),
            "filename": att.filename,
            "storage_key": att.storage_key,
        }))).await;

    Ok(htmx_redirect_owned(format!("/reports/{public_id}")))
}

fn sanitize_filename(name: &str) -> String {
    let base = std::path::Path::new(name)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    base.chars()
        .filter(|c| c.is_alphanumeric() || matches!(*c, '.' | '_' | '-'))
        .take(120)
        .collect()
}

fn human_size(bytes: i64) -> String {
    const KB: i64 = 1024;
    const MB: i64 = KB * 1024;
    if bytes >= MB { format!("{:.1} MB", bytes as f64 / MB as f64) }
    else if bytes >= KB { format!("{:.1} KB", bytes as f64 / KB as f64) }
    else { format!("{bytes} B") }
}

fn render_markdown(md: &str) -> String {
    use pulldown_cmark::{html, Parser};
    let parser = Parser::new(md);
    let mut unsafe_html = String::new();
    html::push_html(&mut unsafe_html, parser);
    ammonia::clean(&unsafe_html)
}

const fn all_states() -> [ReportState; 10] {
    use ReportState::*;
    [New, Triaging, NeedsInfo, Accepted, Duplicate, NotApplicable, Informative, Resolved, Disclosed, Rejected]
}

/// Notifica al reporter si el actor es triager, y a los owners de la company
/// si el actor es el reporter. Best-effort: errores se loggean pero no fallan
/// la request.
async fn notify_other_side(
    state: &AppState,
    ctx: &ReportCtx,
    actor: &CurrentUser,
    public_id: &str,
    what: &str,
) -> Result<(), crate::email::EmailError> {
    let subject = format!("[{public_id}] {what}");
    let text = format!(
        "{}\n\nActor: {}\nReport: {}/reports/{}\n",
        what, actor.user.handle, state.cfg.public_url, public_id
    );
    if ctx.is_triager {
        // triager actuó → notificar al reporter
        let email = Email {
            to: ctx.reporter_email.clone(),
            subject,
            html_body: text.replace('\n', "<br>"),
            text_body: text,
        };
        state.email.send(&email).await?;
    } else if ctx.is_reporter {
        // reporter actuó → notificar a los owners de la company. Necesitamos
        // resolver program → company.
        let company_id = match db::programs::find_by_id(&state.db, ctx.report.program_id).await {
            Ok(Some(p)) => p.company_id,
            _ => return Ok(()),
        };
        send_company_notification(state, company_id, &subject, &text).await?;
    }
    Ok(())
}

/// Manda email a los owners/admins de una company.
async fn send_company_notification(
    state: &AppState,
    company_id: crate::domain::ids::CompanyId,
    subject: &str,
    text: &str,
) -> Result<(), crate::email::EmailError> {
    let recipients = match db::companies::owner_emails(&state.db, company_id).await {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(error = ?e, "could not fetch company owner emails");
            return Ok(());
        }
    };
    for to in recipients {
        let email = Email {
            to,
            subject: subject.to_string(),
            html_body: text.replace('\n', "<br>"),
            text_body: text.to_string(),
        };
        state.email.send(&email).await?;
    }
    Ok(())
}
