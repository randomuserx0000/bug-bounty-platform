//! Composición del router HTTP.
//!
//! Las rutas se agrupan por dominio (auth, programs, reports, payouts...).
//! Cada submódulo expone una función `router()` que devuelve un `Router`
//! tipado con `AppState`, listo para anidar.
//!
//! Middlewares globales (trace, compression, timeouts, static files) se
//! aplican aquí una sola vez. El rate-limit es **selectivo**: solo va
//! sobre el router de auth, donde el brute force es la amenaza real.

use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::extract::Request;
use axum::middleware::Next;
use axum::response::Response;
use axum::routing::get;
use axum::http::{HeaderName, HeaderValue, header};
use tower_governor::governor::GovernorConfigBuilder;
use tower_governor::GovernorLayer;
use tower_http::compression::CompressionLayer;
use tower_http::services::ServeDir;
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;

use crate::state::AppState;

mod admin;
mod assets;
mod auth;
mod companies;
mod dashboard;
mod health;
mod home;
mod oauth;
mod payouts;
mod programs;
mod reports;
mod settings;

/// Añade los headers HTTP de seguridad críticos en cada respuesta.
///
/// - `X-Content-Type-Options: nosniff` impide que el navegador adivine el
///   MIME type y ejecute contenido inesperado (p.ej. un JS camuflado como PNG).
/// - `X-Frame-Options: DENY` bloquea que la app se cargue en un iframe,
///   previniendo ataques de clickjacking.
/// - `Content-Security-Policy` limita de dónde pueden cargarse recursos;
///   `frame-ancestors 'none'` refuerza la protección contra iframes (CSP Lv.2+).
/// - `Strict-Transport-Security` fuerza HTTPS durante un año, incluidos subdominios.
/// - `Referrer-Policy` evita filtrar la URL completa a terceros.
/// - `Permissions-Policy` deshabilita APIs sensibles del navegador que la app
///   no necesita.
async fn security_headers(req: Request, next: Next) -> Response {
    let mut res = next.run(req).await;
    let h = res.headers_mut();

    // MIME-sniffing: el navegador NO debe adivinar el content-type.
    h.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    // Clickjacking: prohibir que la app se embeba en un iframe.
    h.insert(
        header::X_FRAME_OPTIONS,
        HeaderValue::from_static("DENY"),
    );
    // CSP: orígenes permitidos para cada tipo de recurso.
    // unsafe-inline es necesario por los <script> inline de htmx y el editor
    // Markdown (easymde). frame-ancestors refuerza X-Frame-Options para CSP Lv.2.
    h.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(
            "default-src 'self'; \
             script-src 'self' 'unsafe-inline' https://unpkg.com; \
             style-src 'self' 'unsafe-inline' https://fonts.googleapis.com https://unpkg.com; \
             font-src 'self' https://fonts.gstatic.com; \
             img-src 'self' data:; \
             connect-src 'self'; \
             frame-ancestors 'none'; \
             base-uri 'self'; \
             form-action 'self'",
        ),
    );
    // HSTS: fuerza HTTPS durante 1 año (solo efectivo en prod tras un dominio real).
    h.insert(
        header::STRICT_TRANSPORT_SECURITY,
        HeaderValue::from_static("max-age=31536000; includeSubDomains"),
    );
    // Referrer: solo enviar origen (sin path/query) a sitios externos.
    h.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("strict-origin-when-cross-origin"),
    );
    // Permissions-Policy: deshabilitar APIs del navegador que la app no usa.
    h.insert(
        HeaderName::from_static("permissions-policy"),
        HeaderValue::from_static("geolocation=(), microphone=(), camera=(), payment=()"),
    );

    res
}

pub fn router(state: AppState) -> Router {
    // Rate limit por IP para auth: ~1 req/seg, ráfaga de 5.
    // Suficiente para uso humano normal; brute force se rompe en 5 intentos.
    let governor_conf = Arc::new(
        GovernorConfigBuilder::default()
            .per_second(1)
            .burst_size(5)
            .finish()
            .expect("governor config válida"),
    );

    let auth_routes = auth::router().layer(GovernorLayer {
        config: governor_conf,
    });

    Router::new()
        .route("/", get(home::index))
        .route("/healthz", get(health::healthz))
        .route("/readyz", get(health::readyz))
        .merge(auth_routes)
        .merge(dashboard::router())
        .merge(settings::router())
        .merge(companies::router())
        .merge(programs::router())
        .merge(assets::router())
        .merge(reports::router())
        .merge(payouts::router())
        .merge(admin::router())
        .merge(oauth::router())
        // Más adelante:
        // .nest("/reports", reports::router())
        // .nest("/payouts", payouts::router())
        .nest_service("/static", ServeDir::new("static"))
        .with_state(state)
        .layer(axum::middleware::from_fn(security_headers))
        .layer(TraceLayer::new_for_http())
        .layer(CompressionLayer::new())
        .layer(TimeoutLayer::new(Duration::from_secs(30)))
}
