//! Helpers compartidos entre routers HTTP: redirects HTMX, fragments de
//! error, regex de slug. Vive en `web` porque produce HTML.

use askama::Template;
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use once_cell::sync::Lazy;
use regex::Regex;
use time::OffsetDateTime;

use super::templates::{FormErrorPartial, FormOkPartial, PriceTierView, ProgramPublicCardView};
use crate::db::programs::PublicProgramRow;

/// Formatea centavos como dólares con separador de miles: 800000 → "$8.000".
pub fn format_usd(cents: i64) -> String {
    let dollars = (cents / 100).to_string();
    let mut out = String::with_capacity(dollars.len() + 4);
    for (i, ch) in dollars.chars().enumerate() {
        if i > 0 && (dollars.len() - i) % 3 == 0 {
            out.push('.');
        }
        out.push(ch);
    }
    format!("${out}")
}

/// Hasta dos iniciales en mayúscula: "Banco Andino" → "BA", "TeleRed" → "T".
pub fn initials(name: &str) -> String {
    let s: String = name
        .split_whitespace()
        .filter_map(|w| w.chars().next())
        .take(2)
        .collect::<String>()
        .to_uppercase();
    if s.is_empty() { "?".into() } else { s }
}

/// Agrupa un tipo de activo en una categoría corta para listados y filtros.
fn asset_category(asset_type: &str) -> (&'static str, &'static str) {
    match asset_type {
        "web" => ("web", "Web"),
        "api" => ("api", "API"),
        "mobile_android" | "mobile_ios" => ("movil", "Móvil"),
        "infra_host" | "infra_range" | "iot_endpoint" => ("infra", "Infraestructura"),
        "firmware" => ("firmware", "Firmware"),
        "hardware_device" | "radio_signal" => ("hardware", "Hardware"),
        "source_repo" | "package" => ("codigo", "Código"),
        _ => ("otro", "Otro"),
    }
}

/// "Hoy", "Ayer", "Hace 3 días", "Hace 2 semanas", "Hace 4 meses".
pub fn relative_es(when: OffsetDateTime) -> String {
    let days = (OffsetDateTime::now_utc() - when).whole_days().max(0);
    match days {
        0 => "Hoy".into(),
        1 => "Ayer".into(),
        2..=6 => format!("Hace {days} días"),
        7..=13 => "Hace 1 semana".into(),
        14..=29 => format!("Hace {} semanas", days / 7),
        30..=59 => "Hace 1 mes".into(),
        _ => format!("Hace {} meses", days / 30),
    }
}

/// Construye la fila de un programa público para la home y el listado.
pub fn program_card(r: PublicProgramRow) -> ProgramPublicCardView {
    let mut keys: Vec<&'static str> = Vec::new();
    let mut labels: Vec<String> = Vec::new();
    for t in &r.asset_types {
        let (k, l) = asset_category(t);
        if !keys.contains(&k) {
            keys.push(k);
            labels.push(l.to_string());
        }
    }
    let max_cents = r.bounty_critical_cents.or(r.bounty_low_cents).unwrap_or(0) as i64;
    let is_new = r
        .launched_at
        .map(|t| (OffsetDateTime::now_utc() - t).whole_days() < 7)
        .unwrap_or(false);
    ProgramPublicCardView {
        initials: initials(&r.company_name),
        company_slug: r.company_slug,
        company_name: r.company_name,
        program_slug: r.slug,
        name: r.name,
        summary: r.summary.unwrap_or_default(),
        bounty_low: r.bounty_low_cents.map(|c| format_usd(c as i64)).unwrap_or_default(),
        bounty_critical: r.bounty_critical_cents.map(|c| format_usd(c as i64)).unwrap_or_default(),
        max_cents,
        asset_labels: labels,
        asset_keys: keys.join(" "),
        updated_label: relative_es(r.updated_at),
        updated_ts: r.updated_at.unix_timestamp(),
        is_new,
    }
}

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
