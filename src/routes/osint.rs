//! Handlers del producto OSINT.
//!
//! Flujo:
//! - **Investigador** envía un informe OSINT (`/osint/new`), lo ve en
//!   `/osint/mine`. Cobra `price_cents` (base $50) cuando se vende.
//! - **Admin** de la plataforma revisa la cola (`/osint/review`), acepta
//!   fijando el precio de reventa, o rechaza.
//! - **Empresa-objetivo** ve los informes aceptados sobre ella en
//!   `/manage/:company_slug/osint` y los compra (debita su escrow).
//!
//! Gating del cuerpo (`body_md`): solo lo ven el autor, un admin, o un miembro
//! de la empresa que ya lo compró. El resto ve el `summary`.

use askama::Template;
use axum::extract::{Path, State};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Form, Router};
use serde::Deserialize;

use crate::audit;
use crate::auth::CurrentUser;
use crate::db;
use crate::db::osint::NewOsintReport;
use crate::domain::ids::UserId;
use crate::domain::osint::{OsintCategory, OsintReportRecord, OsintStatus};
use crate::domain::pricing::OSINT_BASE_CENTS;
use crate::domain::report::ReportSeverity;
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::web::shared::{current_year, error_fragment, htmx_redirect_owned};
use crate::web::templates::{
    OsintCatalogTemplate, OsintMineTemplate, OsintNewTemplate, OsintReviewTemplate,
    OsintRowView, OsintShowTemplate,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/osint/new", get(new_form))
        .route("/osint", post(create))
        .route("/osint/mine", get(mine))
        .route("/osint/review", get(review))
        .route("/osint/:public_id", get(show))
        .route("/osint/:public_id/accept", post(accept))
        .route("/osint/:public_id/reject", post(reject))
        .route("/osint/:public_id/buy", post(buy))
        .route("/manage/:company_slug/osint", get(catalog))
}

// ----------------------------------------------------------------------------
// CREATE
// ----------------------------------------------------------------------------

async fn new_form(
    State(_state): State<AppState>,
    current: CurrentUser,
) -> AppResult<impl IntoResponse> {
    Ok(OsintNewTemplate {
        year: current_year(),
        handle: current.user.handle,
        categories: OsintCategory::all()
            .iter()
            .map(|c| (c.as_str().to_string(), c.label().to_string()))
            .collect(),
        severities: severity_options(),
        osint_base_usd: OSINT_BASE_CENTS / 100,
    })
}

#[derive(Debug, Deserialize)]
struct CreateForm {
    subject_name: String,
    /// Slug opcional de una empresa registrada (para enlazar el informe).
    subject_company_slug: Option<String>,
    title: String,
    category: String,
    criticality: Option<String>,
    summary: String,
    body_md: String,
}

async fn create(
    State(state): State<AppState>,
    current: CurrentUser,
    Form(form): Form<CreateForm>,
) -> AppResult<Response> {
    if form.subject_name.trim().is_empty() {
        return Ok(error_fragment("indica la empresa objetivo"));
    }
    if form.title.trim().is_empty() {
        return Ok(error_fragment("título requerido"));
    }
    if form.summary.trim().len() < 20 {
        return Ok(error_fragment("el resumen público debe tener al menos 20 caracteres"));
    }
    if form.body_md.trim().len() < 50 {
        return Ok(error_fragment("el informe completo debe tener al menos 50 caracteres"));
    }
    let category = OsintCategory::from_str(&form.category)
        .ok_or_else(|| AppError::Validation("categoría inválida".into()))?;
    let criticality = form
        .criticality
        .as_deref()
        .and_then(ReportSeverity::from_str)
        .unwrap_or(ReportSeverity::None);

    // Enlazar a una empresa registrada si el slug coincide (opcional).
    let subject_company_id = match form.subject_company_slug.as_deref().map(str::trim) {
        Some(slug) if !slug.is_empty() => db::companies::find_by_slug(&state.db, slug)
            .await?
            .map(|c| c.id),
        _ => None,
    };

    let (id, public_id) = db::osint::create(
        &state.db,
        NewOsintReport {
            researcher_id: UserId::from(current.user.id),
            subject_company_id,
            subject_name: form.subject_name.trim(),
            title: form.title.trim(),
            category,
            criticality,
            summary: form.summary.trim(),
            body_md: form.body_md.trim(),
            price_cents: OSINT_BASE_CENTS,
        },
    )
    .await?;

    audit::log(&state.db, audit::AuditEntry::new(audit::OSINT_CREATE)
        .actor(current.user.id).target("osint_report", id)
        .metadata(serde_json::json!({
            "public_id": public_id,
            "category": category.as_str(),
            "subject_name": form.subject_name.trim(),
        }))).await;

    Ok(htmx_redirect_owned(format!("/osint/{public_id}")))
}

// ----------------------------------------------------------------------------
// LISTS
// ----------------------------------------------------------------------------

async fn mine(
    State(state): State<AppState>,
    current: CurrentUser,
) -> AppResult<impl IntoResponse> {
    let rows = db::osint::list_for_researcher(&state.db, UserId::from(current.user.id)).await?;
    Ok(OsintMineTemplate {
        year: current_year(),
        handle: current.user.handle,
        rows: rows.iter().map(row_view).collect(),
    })
}

async fn review(
    State(state): State<AppState>,
    current: CurrentUser,
) -> AppResult<impl IntoResponse> {
    require_admin(&current)?;
    let rows = db::osint::list_for_review(&state.db).await?;
    Ok(OsintReviewTemplate {
        year: current_year(),
        handle: current.user.handle,
        rows: rows.iter().map(row_view).collect(),
    })
}

async fn catalog(
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
    let rows = db::osint::list_catalog_for_company(&state.db, company.id).await?;
    let escrow = db::companies::escrow_balance(&state.db, company.id).await?;
    Ok(OsintCatalogTemplate {
        year: current_year(),
        handle: current.user.handle,
        company_slug: company.slug,
        company_name: company.display_name,
        escrow_usd: format!("${:.2}", escrow as f64 / 100.0),
        rows: rows.iter().map(row_view).collect(),
    })
}

// ----------------------------------------------------------------------------
// SHOW
// ----------------------------------------------------------------------------

async fn show(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(public_id): Path<String>,
) -> AppResult<impl IntoResponse> {
    let r = db::osint::find_by_public_id(&state.db, &public_id)
        .await?
        .ok_or(AppError::NotFound)?;

    let user_id = UserId::from(current.user.id);
    let is_author = r.researcher_id == user_id;
    let is_admin = current.user.role == "admin";

    // ¿El usuario gestiona la empresa-objetivo? (para CTA comprar + ver tras compra)
    let mut manages_subject = false;
    if let Some(cid) = r.subject_company_id {
        if let Some(m) = db::companies::membership(&state.db, cid, user_id).await? {
            manages_subject = m.role.can_manage_programs();
        }
    }
    // ¿Gestiona la empresa que YA compró?
    let mut manages_buyer = false;
    if let Some(cid) = r.sold_to_company_id {
        if let Some(m) = db::companies::membership(&state.db, cid, user_id).await? {
            manages_buyer = m.role.can_manage_programs();
        }
    }

    // Solo el autor, un admin, o la empresa que compró pueden leer el cuerpo.
    let can_see_body = is_author || is_admin || (r.status.is_sold() && manages_buyer);
    // Si no es ninguno de los anteriores ni gestiona la empresa-objetivo, y el
    // informe no está aceptado/vendido, ocultamos su existencia.
    let is_listed = matches!(r.status, OsintStatus::Accepted | OsintStatus::Sold);
    if !can_see_body && !manages_subject && !(is_listed && is_admin) {
        return Err(AppError::NotFound);
    }

    let can_review = is_admin
        && matches!(r.status, OsintStatus::Submitted | OsintStatus::InReview);
    let can_buy = manages_subject && matches!(r.status, OsintStatus::Accepted);

    Ok(OsintShowTemplate {
        year: current_year(),
        handle: current.user.handle,
        public_id: r.public_id.clone(),
        title: r.title.clone(),
        subject_name: r.subject_name.clone(),
        category_label: r.category.label().into(),
        criticality: r.criticality.as_str().into(),
        status: r.status.as_str().into(),
        status_label: r.status.label().into(),
        summary_html: render_markdown(&r.summary),
        body_html: if can_see_body { render_markdown(&r.body_md) } else { String::new() },
        can_see_body,
        price_usd: usd(r.price_cents),
        resale_usd: r.resale_price_cents.map(usd).unwrap_or_default(),
        can_review,
        can_buy,
        created: fmt_date(&r),
    })
}

// ----------------------------------------------------------------------------
// ADMIN: accept / reject
// ----------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct AcceptForm {
    resale_usd: String,
}

async fn accept(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(public_id): Path<String>,
    Form(form): Form<AcceptForm>,
) -> AppResult<Response> {
    require_admin(&current)?;
    let r = db::osint::find_by_public_id(&state.db, &public_id)
        .await?
        .ok_or(AppError::NotFound)?;
    if !matches!(r.status, OsintStatus::Submitted | OsintStatus::InReview) {
        return Ok(error_fragment("solo se acepta un informe enviado o en revisión"));
    }
    let resale_usd: i32 = form
        .resale_usd
        .trim()
        .parse()
        .map_err(|_| AppError::Validation("precio de reventa inválido".into()))?;
    if resale_usd <= 0 {
        return Ok(error_fragment("el precio de reventa debe ser > 0"));
    }
    let resale_cents = resale_usd
        .checked_mul(100)
        .ok_or_else(|| AppError::Validation("overflow".into()))?;

    db::osint::accept(&state.db, r.id, UserId::from(current.user.id), resale_cents).await?;
    audit::log(&state.db, audit::AuditEntry::new(audit::OSINT_ACCEPT)
        .actor(current.user.id).target("osint_report", r.id)
        .metadata(serde_json::json!({
            "public_id": r.public_id,
            "price_cents": r.price_cents,
            "resale_price_cents": resale_cents,
        }))).await;

    // Notificar al investigador (best-effort).
    if let Ok(Some(u)) = db::users::find_by_id(&state.db, r.researcher_id.0).await {
        let _ = state.email.send(&crate::email::Email {
            to: u.email,
            subject: format!("[{}] tu informe OSINT fue aceptado", r.public_id),
            text_body: format!(
                "Tu informe OSINT \"{}\" fue aceptado. Cobrarás USD {:.2} cuando la empresa \
                 lo adquiera. Lo verás en {}/osint/mine",
                r.title,
                r.price_cents as f64 / 100.0,
                state.cfg.public_url,
            ),
            html_body: String::new(),
        }).await;
    }

    Ok(htmx_redirect_owned(format!("/osint/{public_id}")))
}

async fn reject(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(public_id): Path<String>,
) -> AppResult<Response> {
    require_admin(&current)?;
    let r = db::osint::find_by_public_id(&state.db, &public_id)
        .await?
        .ok_or(AppError::NotFound)?;
    if !matches!(r.status, OsintStatus::Submitted | OsintStatus::InReview) {
        return Ok(error_fragment("solo se rechaza un informe enviado o en revisión"));
    }
    db::osint::reject(&state.db, r.id, UserId::from(current.user.id)).await?;
    audit::log(&state.db, audit::AuditEntry::new(audit::OSINT_REJECT)
        .actor(current.user.id).target("osint_report", r.id)
        .metadata(serde_json::json!({ "public_id": r.public_id }))).await;
    Ok(htmx_redirect_owned(format!("/osint/{public_id}")))
}

// ----------------------------------------------------------------------------
// COMPANY: buy
// ----------------------------------------------------------------------------

async fn buy(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(public_id): Path<String>,
) -> AppResult<Response> {
    let r = db::osint::find_by_public_id(&state.db, &public_id)
        .await?
        .ok_or(AppError::NotFound)?;

    // Debe estar aceptado y enlazado a una empresa-objetivo que el user gestione.
    if !matches!(r.status, OsintStatus::Accepted) {
        return Ok(error_fragment("este informe no está disponible para compra"));
    }
    let company_id = r.subject_company_id.ok_or(AppError::NotFound)?;
    let m = db::companies::membership(&state.db, company_id, UserId::from(current.user.id))
        .await?
        .ok_or(AppError::Forbidden)?;
    if !m.role.can_manage_programs() {
        return Err(AppError::Forbidden);
    }
    let resale_cents = r
        .resale_price_cents
        .ok_or_else(|| AppError::Validation("el informe no tiene precio de reventa".into()))?
        as i64;

    // Débito de escrow + marcado sold en una transacción. Escrow insuficiente
    // → CHECK rompe la tx y caemos al mensaje de error.
    let sold = match db::osint::purchase(&state.db, r.id, company_id, resale_cents).await {
        Ok(sold) => sold,
        Err(e) if e.to_string().contains("escrow_balance_cents") => {
            return Ok(error_fragment("escrow insuficiente; deposita saldo primero"));
        }
        Err(e) => return Err(e.into()),
    };
    if !sold {
        return Ok(error_fragment("el informe ya no está disponible"));
    }

    audit::log(&state.db, audit::AuditEntry::new(audit::OSINT_PURCHASE)
        .actor(current.user.id).target("osint_report", r.id)
        .metadata(serde_json::json!({
            "public_id": r.public_id,
            "company_id": company_id.to_string(),
            "resale_price_cents": resale_cents,
            "researcher_price_cents": r.price_cents,
        }))).await;

    // Notificar al investigador que su informe se vendió.
    if let Ok(Some(u)) = db::users::find_by_id(&state.db, r.researcher_id.0).await {
        let _ = state.email.send(&crate::email::Email {
            to: u.email,
            subject: format!("[{}] tu informe OSINT se vendió", r.public_id),
            text_body: format!(
                "¡Buenas noticias! Tu informe OSINT \"{}\" fue adquirido. Te corresponde \
                 USD {:.2}. Coordinaremos el pago a tu método configurado.",
                r.title,
                r.price_cents as f64 / 100.0,
            ),
            html_body: String::new(),
        }).await;
    }

    Ok(htmx_redirect_owned(format!("/osint/{public_id}")))
}

// ----------------------------------------------------------------------------
// helpers
// ----------------------------------------------------------------------------

fn require_admin(current: &CurrentUser) -> AppResult<()> {
    if current.user.role == "admin" {
        Ok(())
    } else {
        Err(AppError::Forbidden)
    }
}

fn severity_options() -> Vec<(String, String)> {
    [
        ReportSeverity::None,
        ReportSeverity::Low,
        ReportSeverity::Medium,
        ReportSeverity::High,
        ReportSeverity::Critical,
    ]
    .iter()
    .map(|s| (s.as_str().to_string(), s.as_str().to_string()))
    .collect()
}

fn row_view(r: &OsintReportRecord) -> OsintRowView {
    OsintRowView {
        public_id: r.public_id.clone(),
        title: r.title.clone(),
        subject_name: r.subject_name.clone(),
        category_label: r.category.label().into(),
        criticality: r.criticality.as_str().into(),
        status: r.status.as_str().into(),
        status_label: r.status.label().into(),
        price_usd: usd(r.price_cents),
        resale_usd: r.resale_price_cents.map(usd).unwrap_or_default(),
        created: fmt_date(r),
    }
}

fn fmt_date(r: &OsintReportRecord) -> String {
    r.created_at
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_default()
}

fn usd(cents: i32) -> String {
    format!("${}", cents / 100)
}

fn render_markdown(md: &str) -> String {
    use pulldown_cmark::{html, Parser};
    let parser = Parser::new(md);
    let mut unsafe_html = String::new();
    html::push_html(&mut unsafe_html, parser);
    ammonia::clean(&unsafe_html)
}
