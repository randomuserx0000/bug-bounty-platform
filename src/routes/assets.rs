//! Handlers de assets. Anidados bajo `/manage/:company_slug/programs/:program_slug`.
//!
//! El form de creación es polimórfico: el `<select>` de `asset_type`
//! dispara `hx-get` a `/manage/.../assets/fields?type=...`, que devuelve
//! los inputs específicos para ese tipo. Mismo patrón que payment methods.

use askama::Template;
use axum::extract::{Path, Query, State};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Form, Router};
use serde::Deserialize;
use serde_json::json;
use sqlx::types::JsonValue;
use uuid::Uuid;

use crate::audit;
use crate::auth::CurrentUser;
use crate::db;
use crate::db::assets::NewAsset;
use crate::domain::asset::{AssetSeverityCap, AssetType};
use crate::domain::ids::{AssetId, UserId};
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::web::shared::{current_year, error_fragment, htmx_redirect_owned};
use crate::web::templates::{AssetFieldsPartial, AssetNewTemplate};

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/manage/:company_slug/programs/:program_slug/assets",
            post(create),
        )
        .route(
            "/manage/:company_slug/programs/:program_slug/assets/new",
            get(new_form),
        )
        .route(
            "/manage/:company_slug/programs/:program_slug/assets/fields",
            get(fields_partial),
        )
        .route(
            "/manage/:company_slug/programs/:program_slug/assets/:id/delete",
            post(delete),
        )
}

async fn new_form(
    State(state): State<AppState>,
    current: CurrentUser,
    Path((company_slug, program_slug)): Path<(String, String)>,
) -> AppResult<impl IntoResponse> {
    let (_company, program) =
        require_manage(&state, &current, &company_slug, &program_slug).await?;

    let types = AssetType::all()
        .iter()
        .map(|t| (t.as_str().to_string(), t.display_name().to_string()))
        .collect();

    Ok(AssetNewTemplate {
        year: current_year(),
        handle: current.user.handle,
        account_role: current.user.role.clone(),
        company_slug,
        program_slug: program.slug,
        program_name: program.name,
        types,
        initial_fields: render_fields(AssetType::Web),
    })
}

#[derive(Deserialize)]
struct FieldsQuery {
    #[serde(rename = "asset_type")]
    asset_type: String,
}

async fn fields_partial(
    _current: CurrentUser,
    Path((_company_slug, _program_slug)): Path<(String, String)>,
    Query(q): Query<FieldsQuery>,
) -> AppResult<impl IntoResponse> {
    let at = AssetType::from_str(&q.asset_type)
        .ok_or_else(|| AppError::Validation("tipo desconocido".into()))?;
    Ok(Html(render_fields(at)))
}

fn render_fields(at: AssetType) -> String {
    AssetFieldsPartial { asset_type: at.as_str().into() }
        .render()
        .unwrap_or_default()
}

#[derive(Debug, Deserialize)]
struct CreateForm {
    asset_type: String,
    label: String,
    in_scope: Option<String>,
    severity_cap: Option<String>,
    notes_md: Option<String>,
    // Campos por tipo — solo se llenan los aplicables.
    url: Option<String>,
    scope: Option<String>,
    base_url: Option<String>,
    openapi: Option<String>,
    package: Option<String>,
    bundle_id: Option<String>,
    min_version: Option<String>,
    sha256: Option<String>,
    fqdn: Option<String>,
    ipv4: Option<String>,
    cidr: Option<String>,
    repo_url: Option<String>,
    commit: Option<String>,
    ecosystem: Option<String>,
    pkg_name: Option<String>,
    pkg_version: Option<String>,
    vendor: Option<String>,
    model: Option<String>,
    version: Option<String>,
    hw_rev: Option<String>,
    interfaces: Option<String>,
    protocol: Option<String>,
    endpoint: Option<String>,
    band: Option<String>,
    modulation: Option<String>,
    device: Option<String>,
    description: Option<String>,
}

async fn create(
    State(state): State<AppState>,
    current: CurrentUser,
    Path((company_slug, program_slug)): Path<(String, String)>,
    Form(form): Form<CreateForm>,
) -> AppResult<Response> {
    let (_company, program) =
        require_manage(&state, &current, &company_slug, &program_slug).await?;

    let Some(asset_type) = AssetType::from_str(&form.asset_type) else {
        return Ok(error_fragment("tipo desconocido"));
    };
    if form.label.trim().is_empty() {
        return Ok(error_fragment("la etiqueta es requerida"));
    }

    let target = match build_target(asset_type, &form) {
        Ok(t) => t,
        Err(msg) => return Ok(error_fragment(msg)),
    };

    let severity_cap = form
        .severity_cap
        .as_deref()
        .and_then(AssetSeverityCap::from_str)
        .unwrap_or(AssetSeverityCap::None);

    let notes = form
        .notes_md
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());

    let asset_id = db::assets::create(
        &state.db,
        NewAsset {
            program_id: program.id,
            asset_type,
            label: form.label.trim(),
            target: &target,
            in_scope: form.in_scope.as_deref() == Some("on"),
            severity_cap,
            notes_md: notes,
        },
    )
    .await?;
    audit::log(&state.db, audit::AuditEntry::new(audit::ASSET_CREATE)
        .actor(current.user.id).target("asset", asset_id)
        .metadata(serde_json::json!({
            "program_id": program.id.to_string(),
            "asset_type": asset_type.as_str(),
            "label": form.label.trim(),
        }))).await;

    Ok(htmx_redirect_owned(format!(
        "/manage/{company_slug}/programs/{program_slug}"
    )))
}

async fn delete(
    State(state): State<AppState>,
    current: CurrentUser,
    Path((company_slug, program_slug, id)): Path<(String, String, Uuid)>,
) -> AppResult<Response> {
    let (_company, program) =
        require_manage(&state, &current, &company_slug, &program_slug).await?;
    db::assets::delete(&state.db, AssetId(id), program.id).await?;
    audit::log(&state.db, audit::AuditEntry::new(audit::ASSET_DELETE)
        .actor(current.user.id).target("asset", id)
        .metadata(serde_json::json!({ "program_id": program.id.to_string() }))).await;
    Ok(htmx_redirect_owned(format!(
        "/manage/{company_slug}/programs/{program_slug}"
    )))
}

// ----------------------------------------------------------------------------
// helpers
// ----------------------------------------------------------------------------

/// Resuelve (company, program) y verifica que el user pueda administrarlos.
async fn require_manage(
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

/// Construye el JSON `target` desde los campos del form según el tipo.
/// Devuelve mensaje de error de UI (no AppError) si falta un campo obligatorio.
fn build_target(at: AssetType, f: &CreateForm) -> Result<JsonValue, &'static str> {
    let req = |o: &Option<String>, err: &'static str| -> Result<String, &'static str> {
        o.as_deref()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .ok_or(err)
    };
    let opt = |o: &Option<String>| -> Option<String> {
        o.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(str::to_string)
    };

    Ok(match at {
        AssetType::Web => json!({
            "url": req(&f.url, "url es requerido")?,
            "scope": opt(&f.scope),
        }),
        AssetType::Api => json!({
            "base_url": req(&f.base_url, "base_url es requerido")?,
            "openapi": opt(&f.openapi),
        }),
        AssetType::MobileAndroid => json!({
            "package": req(&f.package, "package es requerido")?,
            "min_version": opt(&f.min_version),
            "sha256": opt(&f.sha256),
        }),
        AssetType::MobileIos => json!({
            "bundle_id": req(&f.bundle_id, "bundle_id es requerido")?,
            "min_version": opt(&f.min_version),
        }),
        AssetType::InfraHost => {
            let fqdn = opt(&f.fqdn);
            let ipv4 = opt(&f.ipv4);
            if fqdn.is_none() && ipv4.is_none() {
                return Err("indica fqdn o ipv4");
            }
            let mut obj = serde_json::Map::new();
            if let Some(v) = fqdn { obj.insert("fqdn".into(), JsonValue::String(v)); }
            if let Some(v) = ipv4 { obj.insert("ipv4".into(), JsonValue::String(v)); }
            JsonValue::Object(obj)
        }
        AssetType::InfraRange => json!({
            "cidr": req(&f.cidr, "cidr es requerido")?,
        }),
        AssetType::SourceRepo => json!({
            "url": req(&f.repo_url, "repo url es requerido")?,
            "commit": opt(&f.commit),
        }),
        AssetType::Package => json!({
            "ecosystem": req(&f.ecosystem, "ecosystem es requerido (npm, pypi, ...)")?,
            "name": req(&f.pkg_name, "nombre del paquete es requerido")?,
            "version": opt(&f.pkg_version),
        }),
        AssetType::Firmware => json!({
            "vendor": req(&f.vendor, "vendor es requerido")?,
            "model": req(&f.model, "model es requerido")?,
            "version": req(&f.version, "version es requerida")?,
            "sha256": opt(&f.sha256),
        }),
        AssetType::HardwareDevice => {
            let ifaces: Vec<String> = f
                .interfaces
                .as_deref()
                .unwrap_or("")
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            json!({
                "vendor": req(&f.vendor, "vendor es requerido")?,
                "model": req(&f.model, "model es requerido")?,
                "hw_rev": opt(&f.hw_rev),
                "interfaces": ifaces,
            })
        }
        AssetType::IotEndpoint => json!({
            "protocol": req(&f.protocol, "protocol es requerido")?,
            "endpoint": req(&f.endpoint, "endpoint es requerido")?,
            "model": opt(&f.model),
        }),
        AssetType::RadioSignal => json!({
            "band": req(&f.band, "band es requerida")?,
            "modulation": req(&f.modulation, "modulation es requerida")?,
            "device": opt(&f.device),
        }),
        AssetType::Other => json!({
            "description": req(&f.description, "descripción es requerida")?,
        }),
    })
}
