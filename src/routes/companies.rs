//! Handlers de companies. Cualquier user logueado puede crear una; el
//! creador queda como `owner` en `company_members` (en la misma tx).

use axum::extract::{Path, State};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Form, Router};
use serde::Deserialize;

use crate::audit;
use crate::auth::CurrentUser;
use crate::db;
use crate::db::companies::NewCompany;
use crate::domain::ids::UserId;
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::web::templates::{
    CompaniesIndexTemplate, CompaniesNewTemplate, CompanyMembershipView, CompanyShowTemplate,
    ProgramRowView,
};
use crate::web::shared::{current_year, error_fragment, htmx_redirect_owned, slug_re};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/companies", get(index).post(create))
        .route("/companies/new", get(new_form))
        .route("/companies/:slug", get(show))
}

async fn index(State(state): State<AppState>, current: CurrentUser) -> AppResult<impl IntoResponse> {
    let user_id = UserId::from(current.user.id);
    let companies = db::companies::list_for_user(&state.db, user_id).await?;

    let memberships = companies
        .into_iter()
        .map(|(c, role)| CompanyMembershipView {
            slug: c.slug,
            display_name: c.display_name,
            role: role.as_str().into(),
            status: c.status.as_str().into(),
        })
        .collect();

    Ok(CompaniesIndexTemplate {
        year: current_year(),
        handle: current.user.handle,
        memberships,
    })
}

async fn new_form(current: CurrentUser) -> AppResult<impl IntoResponse> {
    Ok(CompaniesNewTemplate {
        year: current_year(),
        handle: current.user.handle,
    })
}

#[derive(Debug, Deserialize)]
struct CreateForm {
    slug: String,
    legal_name: String,
    display_name: String,
    country_code: Option<String>,
    website: Option<String>,
    description: Option<String>,
}

async fn create(
    State(state): State<AppState>,
    current: CurrentUser,
    Form(form): Form<CreateForm>,
) -> AppResult<Response> {
    let slug = form.slug.trim().to_lowercase();
    if !slug_re().is_match(&slug) {
        return Ok(error_fragment("slug solo permite letras, números y guiones (3-40 chars)"));
    }
    if form.legal_name.trim().is_empty() || form.display_name.trim().is_empty() {
        return Ok(error_fragment("nombre legal y nombre público son requeridos"));
    }

    let cc_upper = opt(&form.country_code).map(str::to_uppercase);
    let new = NewCompany {
        slug: &slug,
        legal_name: form.legal_name.trim(),
        display_name: form.display_name.trim(),
        country_code: cc_upper.as_deref(),
        website: opt(&form.website),
        description: opt(&form.description),
    };

    match db::companies::create_with_owner(&state.db, UserId::from(current.user.id), new).await {
        Ok(cid) => {
            audit::log(&state.db, audit::AuditEntry::new(audit::COMPANY_CREATE)
                .actor(current.user.id).target("company", cid)
                .metadata(serde_json::json!({ "slug": slug }))).await;
            Ok(htmx_redirect_owned(format!("/companies/{slug}")))
        }
        Err(sqlx::Error::Database(e)) if e.is_unique_violation() => {
            Ok(error_fragment("ese slug ya está tomado"))
        }
        Err(e) => Err(e.into()),
    }
}

async fn show(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(slug): Path<String>,
) -> AppResult<impl IntoResponse> {
    let company = db::companies::find_by_slug(&state.db, &slug)
        .await?
        .ok_or(AppError::NotFound)?;

    // Acceso: solo miembros ven la página de gestión de su company.
    let user_id = UserId::from(current.user.id);
    let membership = db::companies::membership(&state.db, company.id, user_id)
        .await?
        .ok_or(AppError::Forbidden)?;

    let programs = db::programs::list_for_company(&state.db, company.id).await?;
    let programs = programs
        .into_iter()
        .map(|p| ProgramRowView {
            slug: p.slug,
            name: p.name,
            visibility: p.visibility.as_str().into(),
            status: p.status.as_str().into(),
            summary: p.summary.unwrap_or_default(),
        })
        .collect();

    Ok(CompanyShowTemplate {
        year: current_year(),
        handle: current.user.handle,
        company_slug: company.slug,
        company_name: company.display_name,
        company_description: company.description.unwrap_or_default(),
        company_website: company.website.unwrap_or_default(),
        role: membership.role.as_str().into(),
        can_manage: membership.role.can_manage_programs(),
        programs,
    })
}

fn opt(s: &Option<String>) -> Option<&str> {
    s.as_deref().map(str::trim).filter(|x| !x.is_empty())
}
