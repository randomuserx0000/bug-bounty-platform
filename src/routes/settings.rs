//! Handlers de la sección /settings (perfil del usuario logueado).
//!
//! Por ahora solo aloja métodos de pago. Eventualmente: cambio de password,
//! 2FA, datos KYC, preferencias de notificaciones.

use askama::Template;
use axum::extract::{Path, Query, State};
use axum::response::{Html, IntoResponse};
use axum::routing::{get, post};
use axum::{Form, Router};
use serde::Deserialize;
use uuid::Uuid;

use crate::auth::CurrentUser;
use crate::db;
use crate::db::payment_methods::{NewPaymentMethod, PaymentMethodRow};
use crate::domain::ids::{PaymentMethodId, UserId};
use crate::error::{AppError, AppResult};
use crate::payments::crypto;
use crate::payments::details::{
    BankUsdAccount, BankVesAccount, CryptoAddress, EmailHandle, HandleId, RailDetails,
};
use crate::payments::{PaymentRail, RailError};
use crate::state::AppState;
use crate::web::templates::{
    FormErrorPartial, PaymentMethodFieldsPartial, PaymentMethodView, PaymentMethodsNewTemplate,
    PaymentMethodsTemplate,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/settings/payment-methods", get(list).post(create))
        .route("/settings/payment-methods/new", get(new_form))
        .route("/settings/payment-methods/fields", get(fields_partial))
        .route("/settings/payment-methods/:id/delete", post(delete))
        .route("/settings/payment-methods/:id/default", post(set_default))
}

// ----------------------------------------------------------------------------
// LIST
// ----------------------------------------------------------------------------

async fn list(State(state): State<AppState>, current: CurrentUser) -> AppResult<impl IntoResponse> {
    let user_id = UserId::from(current.user.id);
    let rows = db::payment_methods::list_for_user(&state.db, user_id).await?;

    let methods = rows
        .into_iter()
        .map(|r| decode_row_for_view(&state.pm_key, r))
        .collect();

    Ok(PaymentMethodsTemplate {
        year: current_year(),
        handle: current.user.handle,
        methods,
    })
}

fn decode_row_for_view(key: &[u8; 32], r: PaymentMethodRow) -> PaymentMethodView {
    let handler = r.rail.handler();

    // Si el blob está corrupto o la key no cuadra, mostramos algo neutro
    // en vez de tumbar la página completa.
    let short = match crypto::decrypt(key, &r.details_enc)
        .ok()
        .and_then(|pt| serde_json::from_slice::<RailDetails>(&pt).ok())
    {
        Some(d) => handler.short_display(&d),
        None => "(no se pudo leer)".into(),
    };

    PaymentMethodView {
        id: r.id.to_string(),
        rail_label: r.rail.display_name().into(),
        rail_slug: r.rail.as_str().into(),
        label: r.label.unwrap_or_default(),
        short_display: short,
        is_default: r.is_default,
    }
}

// ----------------------------------------------------------------------------
// NEW (form vacío)
// ----------------------------------------------------------------------------

async fn new_form(current: CurrentUser) -> AppResult<impl IntoResponse> {
    Ok(PaymentMethodsNewTemplate {
        year: current_year(),
        handle: current.user.handle,
        rails: ALL_RAILS
            .iter()
            .map(|r| (r.as_str().to_string(), r.display_name().to_string()))
            .collect(),
        // Empezamos con el shape de USDT TRC20 cargado (el más usado en VE).
        initial_fields: render_fields(PaymentRail::UsdtTrc20),
    })
}

// ----------------------------------------------------------------------------
// FIELDS PARTIAL — HTMX dispara este endpoint cuando cambia el <select>
// ----------------------------------------------------------------------------

#[derive(Deserialize)]
struct FieldsQuery {
    rail: String,
}

async fn fields_partial(
    _current: CurrentUser,
    Query(q): Query<FieldsQuery>,
) -> AppResult<impl IntoResponse> {
    let rail = parse_rail(&q.rail)
        .ok_or_else(|| AppError::Validation("rail desconocido".into()))?;
    Ok(Html(render_fields(rail)))
}

fn render_fields(rail: PaymentRail) -> String {
    PaymentMethodFieldsPartial { rail: rail.as_str().into() }
        .render()
        .unwrap_or_default()
}

// ----------------------------------------------------------------------------
// CREATE
// ----------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct CreateForm {
    rail: String,
    label: Option<String>,
    is_default: Option<String>,
    // Campos planos — solo los que aplican al rail llegarán poblados.
    address: Option<String>,
    memo: Option<String>,
    bank_name: Option<String>,
    account_number: Option<String>,
    routing_or_swift: Option<String>,
    holder_name: Option<String>,
    country_code: Option<String>,
    bank_code: Option<String>,
    holder_id: Option<String>,
    email: Option<String>,
    handle: Option<String>,
}

async fn create(
    State(state): State<AppState>,
    current: CurrentUser,
    Form(form): Form<CreateForm>,
) -> AppResult<axum::response::Response> {
    let Some(rail) = parse_rail(&form.rail) else {
        return Ok(error_fragment("rail desconocido"));
    };

    let details = match build_details(rail, &form) {
        Ok(d) => d,
        Err(AppError::Validation(m)) => return Ok(error_fragment(&m)),
        Err(e) => return Err(e),
    };
    if let Err(e) = rail.handler().validate(&details) {
        return Ok(error_fragment(&validation_message(&e)));
    }

    let plaintext = serde_json::to_vec(&details)
        .map_err(|e| anyhow::anyhow!("serializar details: {e}"))?;
    let blob = crypto::encrypt(&state.pm_key, &plaintext)
        .map_err(|e| anyhow::anyhow!("cifrar details: {e}"))?;

    let label_trimmed = form.label.as_deref().map(str::trim).filter(|s| !s.is_empty());

    let pm_id = db::payment_methods::create(
        &state.db,
        NewPaymentMethod {
            user_id: UserId::from(current.user.id),
            rail,
            label: label_trimmed,
            details_enc: &blob,
            is_default: form.is_default.as_deref() == Some("on"),
        },
    )
    .await?;
    crate::audit::log(&state.db, crate::audit::AuditEntry::new(crate::audit::PM_CREATE)
        .actor(current.user.id).target("payment_method", pm_id)
        .metadata(serde_json::json!({
            "rail": rail.as_str(),
            "is_default": form.is_default.as_deref() == Some("on"),
        }))).await;

    // HTMX: redirigir a la lista (HX-Redirect lo intercepta).
    Ok(htmx_redirect("/settings/payment-methods"))
}

fn build_details(rail: PaymentRail, f: &CreateForm) -> AppResult<RailDetails> {
    use AppError::Validation as V;
    let s = |o: &Option<String>| -> AppResult<String> {
        o.as_deref()
            .map(|x| x.trim().to_string())
            .filter(|x| !x.is_empty())
            .ok_or_else(|| V("falta un campo requerido".into()))
    };
    Ok(match rail {
        PaymentRail::UsdtTrc20 | PaymentRail::UsdtErc20 | PaymentRail::Btc => {
            RailDetails::Crypto(CryptoAddress {
                address: s(&f.address)?,
                memo: f.memo.as_deref().map(str::trim).filter(|m| !m.is_empty()).map(str::to_string),
            })
        }
        PaymentRail::BankUsd => RailDetails::BankUsd(BankUsdAccount {
            bank_name: s(&f.bank_name)?,
            account_number: s(&f.account_number)?,
            routing_or_swift: s(&f.routing_or_swift)?,
            holder_name: s(&f.holder_name)?,
            country_code: s(&f.country_code)?.to_uppercase(),
        }),
        PaymentRail::BankVesSudeban => RailDetails::BankVes(BankVesAccount {
            bank_code: s(&f.bank_code)?,
            account_number: s(&f.account_number)?,
            holder_name: s(&f.holder_name)?,
            holder_id: s(&f.holder_id)?.to_uppercase(),
        }),
        PaymentRail::Paypal => RailDetails::Email(EmailHandle { email: s(&f.email)? }),
        PaymentRail::BinancePay | PaymentRail::Zinli => {
            RailDetails::Handle(HandleId { handle: s(&f.handle)? })
        }
    })
}

// ----------------------------------------------------------------------------
// DELETE / SET DEFAULT
// ----------------------------------------------------------------------------

async fn delete(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(id): Path<Uuid>,
) -> AppResult<axum::response::Response> {
    let user_id = UserId::from(current.user.id);
    db::payment_methods::delete(&state.db, PaymentMethodId(id), user_id).await?;
    crate::audit::log(&state.db, crate::audit::AuditEntry::new(crate::audit::PM_DELETE)
        .actor(current.user.id).target("payment_method", id)).await;
    Ok(htmx_redirect("/settings/payment-methods"))
}

async fn set_default(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(id): Path<Uuid>,
) -> AppResult<axum::response::Response> {
    let user_id = UserId::from(current.user.id);
    db::payment_methods::set_default(&state.db, PaymentMethodId(id), user_id).await?;
    crate::audit::log(&state.db, crate::audit::AuditEntry::new(crate::audit::PM_SET_DEFAULT)
        .actor(current.user.id).target("payment_method", id)).await;
    Ok(htmx_redirect("/settings/payment-methods"))
}

// ----------------------------------------------------------------------------
// helpers
// ----------------------------------------------------------------------------

const ALL_RAILS: [PaymentRail; 8] = [
    PaymentRail::UsdtTrc20,
    PaymentRail::UsdtErc20,
    PaymentRail::Btc,
    PaymentRail::BankUsd,
    PaymentRail::BankVesSudeban,
    PaymentRail::Paypal,
    PaymentRail::BinancePay,
    PaymentRail::Zinli,
];

fn parse_rail(s: &str) -> Option<PaymentRail> {
    ALL_RAILS.iter().copied().find(|r| r.as_str() == s)
}

fn htmx_redirect(to: &'static str) -> axum::response::Response {
    use axum::http::{HeaderMap, HeaderValue, StatusCode};
    // Todos los POSTs de esta sección vienen por HTMX: HX-Redirect basta.
    let mut headers = HeaderMap::new();
    headers.insert("HX-Redirect", HeaderValue::from_static(to));
    (StatusCode::OK, headers, "").into_response()
}

fn current_year() -> i32 {
    time::OffsetDateTime::now_utc().year()
}

fn validation_message(e: &RailError) -> String {
    e.to_string()
}

fn error_fragment(msg: &str) -> axum::response::Response {
    use askama::Template;
    let body = FormErrorPartial { message: msg.into() }
        .render()
        .unwrap_or_else(|_| String::from("<div class=\"alert alert-error\">error</div>"));
    Html(body).into_response()
}
