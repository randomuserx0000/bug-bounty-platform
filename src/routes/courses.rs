//! Curso "Analista de Ciberseguridad" (academia).
//!
//! v1: landing pública + formulario "Solicitar curso" (captación de leads).
//! El LMS completo (módulos, evaluación, certificado) está diseñado en
//! docs/osint-academy.md y queda para una fase posterior; mientras tanto las
//! solicitudes quedan en `course_requests` y avisan al buzón de la plataforma.

use std::sync::Arc;
use std::time::Duration;

use axum::extract::State;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Form, Router};
use serde::Deserialize;
use tower_governor::governor::GovernorConfigBuilder;
use tower_governor::key_extractor::SmartIpKeyExtractor;
use tower_governor::GovernorLayer;

use crate::auth::MaybeUser;
use crate::db;
use crate::db::courses::NewCourseRequest;
use crate::domain::ids::UserId;
use crate::domain::pricing::COURSE_ANALISTA_CENTS;
use crate::error::AppResult;
use crate::state::AppState;
use crate::web::shared::{current_year, error_fragment, ok_fragment};
use crate::web::templates::{CourseCatalogTemplate, CourseAnalistaTemplate};

/// Único curso de la v1. Cuando haya más, esto pasa a la tabla `courses`.
const COURSE_SLUG: &str = "analista-ciberseguridad";

pub fn router() -> Router<AppState> {
    // Rate-limit anti-spam sobre la solicitud: el form es PÚBLICO (sin login),
    // por IP de cliente, ráfaga de 3 y luego 1 cada 60s. Misma decisión que el
    // envío OSINT (ver routes/osint.rs) y misma nota de confianza del proxy.
    let submit_governor = Arc::new(
        GovernorConfigBuilder::default()
            .period(Duration::from_secs(60))
            .burst_size(3)
            .key_extractor(SmartIpKeyExtractor)
            .finish()
            .expect("governor config cursos válida"),
    );
    let submit_route = Router::new()
        .route("/cursos/solicitar", post(request_course))
        .layer(GovernorLayer { config: submit_governor });

    Router::new()
        .route("/cursos", get(catalog))
        .route("/cursos/analista-ciberseguridad", get(landing))
        .merge(submit_route)
}

/// Catálogo público de cursos. Muestra el Analista (disponible) + las 4
/// capacitaciones individuales (próximamente).
async fn catalog(
    State(_state): State<AppState>,
    MaybeUser(user): MaybeUser,
) -> AppResult<impl IntoResponse> {
    Ok(CourseCatalogTemplate {
        year: current_year(),
        account_role: user.as_ref().map(|u| u.role.clone()).unwrap_or_default(),
        handle: user.map(|u| u.handle).unwrap_or_default(),
        price_analista: COURSE_ANALISTA_CENTS / 100,
    })
}

/// Landing pública del curso. Sin auth: el público objetivo todavía no tiene
/// cuenta ("fórmate desde cero").
async fn landing(
    State(_state): State<AppState>,
    MaybeUser(user): MaybeUser,
) -> AppResult<impl IntoResponse> {
    Ok(CourseAnalistaTemplate {
        year: current_year(),
        account_role: user.as_ref().map(|u| u.role.clone()).unwrap_or_default(),
        prefill_name: user
            .as_ref()
            .and_then(|u| u.display_name.clone())
            .unwrap_or_default(),
        prefill_email: user.as_ref().map(|u| u.email.clone()).unwrap_or_default(),
        handle: user.map(|u| u.handle).unwrap_or_default(),
        price_usd: COURSE_ANALISTA_CENTS / 100,
    })
}

#[derive(Debug, Deserialize)]
struct RequestForm {
    name: String,
    email: String,
    experience: Option<String>,
    message: Option<String>,
    /// Honeypot: campo oculto por CSS que un humano nunca llena.
    website: Option<String>,
}

async fn request_course(
    State(state): State<AppState>,
    MaybeUser(user): MaybeUser,
    Form(form): Form<RequestForm>,
) -> AppResult<Response> {
    // Bot que llenó el honeypot: respondemos "ok" sin guardar nada, para no
    // darle señal de que fue detectado.
    if form.website.as_deref().is_some_and(|w| !w.trim().is_empty()) {
        return Ok(confirmation());
    }

    let name = form.name.trim();
    let email = form.email.trim();
    if name.len() < 3 {
        return Ok(error_fragment("dinos tu nombre completo"));
    }
    if email.len() < 6 || !email.contains('@') || email.contains(' ') {
        return Ok(error_fragment("correo inválido"));
    }
    // Solo valores conocidos; cualquier otra cosa cae al default.
    let experience = match form.experience.as_deref() {
        Some("basic") => "basic",
        Some("intermediate") => "intermediate",
        _ => "none",
    };
    let message = form.message.as_deref().unwrap_or("").trim();
    if message.len() > 2000 {
        return Ok(error_fragment("el mensaje es demasiado largo (máx 2000 caracteres)"));
    }

    db::courses::create_request(
        &state.db,
        NewCourseRequest {
            user_id: user.as_ref().map(|u| UserId::from(u.id)),
            name,
            email,
            experience,
            message,
            course_slug: COURSE_SLUG,
        },
    )
    .await?;

    // Aviso al buzón de la plataforma (el mismo de OSINT; hace de buzón
    // operativo general). Best-effort: la solicitud ya quedó en BD.
    if let Some(to) = state.cfg.osint_notify_email() {
        let body = format!(
            "Nueva solicitud del curso Analista de Ciberseguridad.\n\n\
             Nombre: {name}\n\
             Email: {email}\n\
             Experiencia: {experience}\n\
             Cuenta en plataforma: {account}\n\n\
             Mensaje:\n{message}\n",
            account = user
                .as_ref()
                .map(|u| u.handle.as_str())
                .unwrap_or("(sin cuenta)"),
        );
        let _ = state
            .email
            .send(&crate::email::Email {
                to: to.to_string(),
                subject: format!("[CURSO] solicitud de {name} ({email})"),
                text_body: body.clone(),
                html_body: body.replace('\n', "<br>"),
            })
            .await;
    }

    Ok(confirmation())
}

fn confirmation() -> Response {
    ok_fragment(
        "¡Solicitud recibida! Te escribiremos a tu correo con los pasos de \
         inscripción y el inicio de la próxima cohorte.",
    )
}
