//! Helpers compartidos entre routers HTTP: redirects HTMX, fragments de
//! error, regex de slug. Vive en `web` porque produce HTML.

use askama::Template;
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use once_cell::sync::Lazy;
use regex::Regex;
use time::OffsetDateTime;

use super::templates::{FormErrorPartial, FormOkPartial, PriceTierView};

pub fn current_year() -> i32 {
    OffsetDateTime::now_utc().year()
}

/// Construye las filas de precio por severidad desde `domain::pricing`.
/// Reutilizado por la home y el form de programa.
pub fn severity_tier_views() -> Vec<PriceTierView> {
    crate::domain::pricing::SEVERITY_TIERS
        .iter()
        .map(|t| PriceTierView {
            emoji: t.emoji.into(),
            label: t.label.into(),
            range: t.range_usd(),
            default_usd: t.min_usd(),
            key: t.key.into(),
        })
        .collect()
}

/// 200 + `HX-Redirect`. HTMX lo intercepta y navega.
pub fn htmx_redirect(to: &'static str) -> Response {
    let mut headers = HeaderMap::new();
    headers.insert("HX-Redirect", HeaderValue::from_static(to));
    (StatusCode::OK, headers, "").into_response()
}

pub fn htmx_redirect_owned(to: String) -> Response {
    let mut headers = HeaderMap::new();
    if let Ok(v) = HeaderValue::from_str(&to) {
        headers.insert("HX-Redirect", v);
    }
    (StatusCode::OK, headers, "").into_response()
}

/// 200 con un fragment `FormErrorPartial`. HTMX lo inyecta en `#form-feedback`.
/// Devolvemos 200 (no 4xx) porque HTMX por defecto no swap-ea respuestas
/// non-2xx. Misma decisión que en routes/auth.rs.
pub fn error_fragment(msg: &str) -> Response {
    let body = FormErrorPartial { message: msg.into() }
        .render()
        .unwrap_or_else(|_| String::from("<div class=\"alert alert-error\">error</div>"));
    Html(body).into_response()
}

/// 200 con un fragment `FormOkPartial`. Contraparte de éxito de
/// `error_fragment`, para forms cuyo final feliz no es un redirect
/// (p.ej. la solicitud de curso).
pub fn ok_fragment(msg: &str) -> Response {
    let body = FormOkPartial { message: msg.into() }
        .render()
        .unwrap_or_else(|_| String::from("<div class=\"alert alert-ok\">listo</div>"));
    Html(body).into_response()
}

/// Regex de slug para companies y programs: 3-40 chars, lowercase
/// alfanumérico + guiones, no puede empezar/terminar con guión.
pub fn slug_re() -> &'static Regex {
    // 3-40 chars total. Empieza y termina alfanumérico, interno permite guión.
    static RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r"^[a-z0-9][a-z0-9-]{1,38}[a-z0-9]$").expect("regex válido")
    });
    &RE
}
