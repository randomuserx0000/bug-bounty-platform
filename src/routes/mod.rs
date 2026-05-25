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
use axum::routing::get;
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
        .layer(TraceLayer::new_for_http())
        .layer(CompressionLayer::new())
        .layer(TimeoutLayer::new(Duration::from_secs(30)))
}
