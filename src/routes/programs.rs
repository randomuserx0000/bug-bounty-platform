//! Handlers de programs.
//!
//! - `/programs` y `/programs/:company_slug/:program_slug` son públicos
//!   (no exigen login): es el storefront del bug bounty.
//! - `/companies/:slug/programs/new` y el POST a `/companies/:slug/programs`
//!   requieren login + membresía con permisos de admin/owner.

use axum::extract::{Path, State};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Form, Router};
use serde::Deserialize;

use crate::audit;
use crate::auth::{CurrentUser, MaybeUser};
use crate::db;
use crate::db::programs::NewProgram;
use crate::domain::asset::summarize_target;
use crate::domain::ids::UserId;
use crate::domain::program::{ProgramStatus, ProgramVisibility};
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::web::shared::{current_year, error_fragment, htmx_redirect_owned, slug_re};
use crate::web::templates::{
    AssetRowView, ProgramNewTemplate, ProgramPublicCardView, ProgramShowTemplate,
    ProgramsPublicTemplate,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/programs", get(public_index))
        .route(
            "/programs/:company_slug/:program_slug",
            get(public_show),
        )
        .route(
            "/companies/:company_slug/programs",
            axum::routing::post(create),
        )
        .route("/companies/:company_slug/programs/new", get(new_form))
        .route(
            "/manage/:company_slug/programs/:program_slug",
            get(manage_show),
        )
}

// ----------------------------------------------------------------------------
// Lista pública (sin auth)
// ----------------------------------------------------------------------------

async fn public_index(
    State(state): State<AppState>,
    MaybeUser(user): MaybeUser,
) -> AppResult<impl IntoResponse> {
    let rows = db::programs::list_public(&state.db).await?;
    let cards = rows
        .into_iter()
        .map(|r| ProgramPublicCardView {
            company_slug: r.company_slug,
            company_name: r.company_name,
            program_slug: r.slug,
            name: r.name,
            summary: r.summary.unwrap_or_default(),
            bounty_low: r.bounty_low_cents.map(usd).unwrap_or_default(),
            bounty_critical: r.bounty_critical_cents.map(usd).unwrap_or_default(),
        })
        .collect();
    Ok(ProgramsPublicTemplate {
        year: current_year(),
        programs: cards,
        handle: user.map(|u| u.handle).unwrap_or_default(),
    })
}

async fn public_show(
    State(state): State<AppState>,
    MaybeUser(user): MaybeUser,
    Path((company_slug, program_slug)): Path<(String, String)>,
) -> AppResult<impl IntoResponse> {
    let company = db::companies::find_by_slug(&state.db, &company_slug)
        .await?
        .ok_or(AppError::NotFound)?;
    let program = db::programs::find_by_company_and_slug(&state.db, company.id, &program_slug)
        .await?
        .ok_or(AppError::NotFound)?;

    // Si no es público en ambos ejes, escondemos su existencia.
    let is_public = matches!(program.visibility, ProgramVisibility::Public)
        && matches!(program.status, ProgramStatus::Public);
    if !is_public {
        return Err(AppError::NotFound);
    }

    let assets = db::assets::list_for_program(&state.db, program.id).await?;
    let asset_rows = assets
        .into_iter()
        .map(|a| AssetRowView {
            id: a.id.to_string(),
            asset_type: a.asset_type.as_str().into(),
            type_label: a.asset_type.display_name().into(),
            label: a.label,
            target: summarize_target(a.asset_type, &a.target),
            in_scope: a.in_scope,
            severity_cap: a.severity_cap.as_str().into(),
        })
        .collect();

    Ok(ProgramShowTemplate {
        year: current_year(),
        public_view: true,
        can_manage: false,
        company_slug: company.slug,
        company_name: company.display_name,
        program_slug: program.slug,
        program_name: program.name,
        summary: program.summary.unwrap_or_default(),
        policy_html: render_markdown(&program.policy_md),
        visibility: program.visibility.as_str().into(),
        status: program.status.as_str().into(),
        bounty_low: program.bounty_low_cents.map(usd).unwrap_or_default(),
        bounty_medium: program.bounty_medium_cents.map(usd).unwrap_or_default(),
        bounty_high: program.bounty_high_cents.map(usd).unwrap_or_default(),
        bounty_critical: program.bounty_critical_cents.map(usd).unwrap_or_default(),
        assets: asset_rows,
        handle: user.map(|u| u.handle).unwrap_or_default(),
    })
}

// ----------------------------------------------------------------------------
// Vista de gestión (autenticada, mismo layout que la pública pero con
// botones de admin y assets editables).
// ----------------------------------------------------------------------------

async fn manage_show(
    State(state): State<AppState>,
    current: CurrentUser,
    Path((company_slug, program_slug)): Path<(String, String)>,
) -> AppResult<impl IntoResponse> {
    let company = db::companies::find_by_slug(&state.db, &company_slug)
        .await?
        .ok_or(AppError::NotFound)?;
    let m = db::companies::membership(&state.db, company.id, UserId::from(current.user.id))
        .await?
        .ok_or(AppError::Forbidden)?;
    let program = db::programs::find_by_company_and_slug(&state.db, company.id, &program_slug)
        .await?
        .ok_or(AppError::NotFound)?;

    let assets = db::assets::list_for_program(&state.db, program.id).await?;
    let asset_rows = assets
        .into_iter()
        .map(|a| AssetRowView {
            id: a.id.to_string(),
            asset_type: a.asset_type.as_str().into(),
            type_label: a.asset_type.display_name().into(),
            label: a.label,
            target: summarize_target(a.asset_type, &a.target),
            in_scope: a.in_scope,
            severity_cap: a.severity_cap.as_str().into(),
        })
        .collect();

    Ok(ProgramShowTemplate {
        year: current_year(),
        public_view: false,
        can_manage: m.role.can_manage_programs(),
        company_slug: company.slug,
        company_name: company.display_name,
        program_slug: program.slug,
        program_name: program.name,
        summary: program.summary.unwrap_or_default(),
        policy_html: render_markdown(&program.policy_md),
        visibility: program.visibility.as_str().into(),
        status: program.status.as_str().into(),
        bounty_low: program.bounty_low_cents.map(usd).unwrap_or_default(),
        bounty_medium: program.bounty_medium_cents.map(usd).unwrap_or_default(),
        bounty_high: program.bounty_high_cents.map(usd).unwrap_or_default(),
        bounty_critical: program.bounty_critical_cents.map(usd).unwrap_or_default(),
        assets: asset_rows,
        handle: current.user.handle,
    })
}

// ----------------------------------------------------------------------------
// Crear program (autenticado, con permisos)
// ----------------------------------------------------------------------------

async fn new_form(
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

    Ok(ProgramNewTemplate {
        year: current_year(),
        handle: current.user.handle,
        company_slug: company.slug,
        company_name: company.display_name,
    })
}

#[derive(Debug, Deserialize)]
struct CreateForm {
    slug: String,
    name: String,
    summary: Option<String>,
    policy_md: String,
    visibility: String,
    status: String,
    bounty_low: Option<String>,
    bounty_medium: Option<String>,
    bounty_high: Option<String>,
    bounty_critical: Option<String>,
    allows_redteam: Option<String>,
    allows_hardware: Option<String>,
}

async fn create(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(company_slug): Path<String>,
    Form(form): Form<CreateForm>,
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

    let slug = form.slug.trim().to_lowercase();
    if !slug_re().is_match(&slug) {
        return Ok(error_fragment("slug solo permite letras, números y guiones (3-40 chars)"));
    }
    if form.name.trim().is_empty() || form.policy_md.trim().is_empty() {
        return Ok(error_fragment("name y policy_md son requeridos"));
    }

    let visibility = match form.visibility.as_str() {
        "private" => ProgramVisibility::Private,
        "invite_only" => ProgramVisibility::InviteOnly,
        "public" => ProgramVisibility::Public,
        _ => return Ok(error_fragment("visibilidad inválida")),
    };
    let status = match form.status.as_str() {
        "draft" => ProgramStatus::Draft,
        "private" => ProgramStatus::Private,
        "public" => ProgramStatus::Public,
        "paused" => ProgramStatus::Paused,
        "closed" => ProgramStatus::Closed,
        _ => return Ok(error_fragment("status inválido")),
    };

    let result = db::programs::create(
        &state.db,
        NewProgram {
            company_id: company.id,
            slug: &slug,
            name: form.name.trim(),
            summary: opt(&form.summary),
            policy_md: form.policy_md.trim(),
            visibility,
            status,
            bounty_low_cents: parse_usd(form.bounty_low.as_deref()),
            bounty_medium_cents: parse_usd(form.bounty_medium.as_deref()),
            bounty_high_cents: parse_usd(form.bounty_high.as_deref()),
            bounty_critical_cents: parse_usd(form.bounty_critical.as_deref()),
            allows_redteam: form.allows_redteam.as_deref() == Some("on"),
            allows_hardware: form.allows_hardware.as_deref() == Some("on"),
        },
    )
    .await;

    match result {
        Ok(pid) => {
            audit::log(&state.db, audit::AuditEntry::new(audit::PROGRAM_CREATE)
                .actor(current.user.id).target("program", pid)
                .metadata(serde_json::json!({
                    "slug": slug, "company_id": company.id.to_string(),
                    "visibility": visibility.as_str(), "status": status.as_str(),
                }))).await;
            Ok(htmx_redirect_owned(format!(
                "/manage/{}/programs/{}",
                company.slug, slug
            )))
        }
        Err(sqlx::Error::Database(e)) if e.is_unique_violation() => {
            Ok(error_fragment("ese slug ya existe en esta company"))
        }
        Err(e) => Err(e.into()),
    }
}

// ----------------------------------------------------------------------------
// helpers
// ----------------------------------------------------------------------------

fn opt(s: &Option<String>) -> Option<&str> {
    s.as_deref().map(str::trim).filter(|x| !x.is_empty())
}

/// Parsea "1500" (USD) → 150000 (cents). Acepta vacío como None.
fn parse_usd(s: Option<&str>) -> Option<i32> {
    let v = s?.trim();
    if v.is_empty() {
        return None;
    }
    v.parse::<i32>().ok().and_then(|usd| usd.checked_mul(100))
}

/// Formatea cents → "$X,XXX" para display.
fn usd(cents: i32) -> String {
    format!("${}", cents / 100)
}

/// Markdown sanitizado a HTML. Defensive defaults: solo tags seguros, sin
/// scripts ni links a `javascript:`.
fn render_markdown(md: &str) -> String {
    use pulldown_cmark::{html, Parser};
    let parser = Parser::new(md);
    let mut unsafe_html = String::new();
    html::push_html(&mut unsafe_html, parser);
    ammonia::clean(&unsafe_html)
}
