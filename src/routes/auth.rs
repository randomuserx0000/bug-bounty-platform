//! Handlers de autenticación: login, signup, logout.
//!
//! Todos los POST de este módulo van detrás de rate-limit por IP
//! (configurado en `routes/mod.rs`). Sin eso, el login es vulnerable
//! a brute force y signup a creación masiva de cuentas para spam.

use axum::extract::{ConnectInfo, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Form, Router};
use axum_extra::extract::cookie::{Cookie, SameSite, SignedCookieJar};
use serde::Deserialize;
use std::net::SocketAddr;
use time::OffsetDateTime;
use validator::Validate;

use sha2::Digest as _;

use crate::audit;
use crate::auth::{self, SESSION_COOKIE};
use crate::db;
use crate::error::AppResult;
use crate::state::AppState;
use crate::web::templates::{FormErrorPartial, LoginTemplate, SignupPendingTemplate, SignupTemplate};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/login", get(login_form).post(login_submit))
        .route("/signup", get(signup_form).post(signup_submit))
        .route("/signup/pending", get(signup_pending))
        .route("/verify-email", get(verify_email))
        .route("/logout", axum::routing::post(logout))
}

// ---------- login ----------

#[derive(Debug, Deserialize, Default)]
struct LoginQuery {
    #[serde(default)]
    next: Option<String>,
}

async fn login_form(
    State(state): State<AppState>,
    crate::auth::MaybeUser(user): crate::auth::MaybeUser,
    axum::extract::Query(q): axum::extract::Query<LoginQuery>,
) -> AppResult<impl IntoResponse> {
    let next = sanitize_next(q.next.as_deref()).unwrap_or_default();
    Ok(LoginTemplate {
        year: current_year(),
        next,
        google_enabled: state.cfg.google_oauth_enabled(),
        account_role: user.as_ref().map(|u| u.role.clone()).unwrap_or_default(),
        handle: user.map(|u| u.handle).unwrap_or_default(),
    })
}

#[derive(Debug, Deserialize, Validate)]
struct LoginForm {
    #[validate(email(message = "email inválido"))]
    email: String,
    #[validate(length(min = 1, message = "contraseña requerida"))]
    password: String,
    /// URL adonde redirigir tras login exitoso. Llega por hidden input
    /// pre-rellenado desde el query string `?next=...`.
    #[serde(default)]
    next: Option<String>,
}

async fn login_submit(
    State(state): State<AppState>,
    jar: SignedCookieJar,
    ConnectInfo(remote): ConnectInfo<SocketAddr>,
    headers_in: HeaderMap,
    Form(form): Form<LoginForm>,
) -> AppResult<axum::response::Response> {
    if form.validate().is_err() {
        return Ok(error_fragment("datos inválidos"));
    }

    let user = db::users::find_by_email(&state.db, &form.email).await?;
    let stored_hash = user.as_ref().map(|u| u.password_hash.as_str());
    let password_ok = auth::verify_password_or_dummy(stored_hash, &form.password);

    let Some(user) = user else {
        audit::log(&state.db, audit::AuditEntry::new(audit::USER_LOGIN_FAILED)
            .ip(remote.ip())
            .metadata(serde_json::json!({ "reason": "no_such_user", "email_attempted": form.email })))
            .await;
        return Ok(error_fragment("credenciales inválidas"));
    };
    if !password_ok || user.status != "active" {
        audit::log(&state.db, audit::AuditEntry::new(audit::USER_LOGIN_FAILED)
            .actor(user.id).ip(remote.ip())
            .metadata(serde_json::json!({
                "reason": if !password_ok { "bad_password" } else { "user_not_active" }
            })))
            .await;
        return Ok(error_fragment("credenciales inválidas"));
    }

    let cookie = issue_session(&state, &user.id, &remote, &headers_in).await?;
    audit::log(&state.db, audit::AuditEntry::new(audit::USER_LOGIN)
        .actor(user.id).ip(remote.ip())
        .target("user", user.id)).await;
    let jar = jar.add(cookie);
    let dest = sanitize_next(form.next.as_deref()).unwrap_or("/dashboard".to_string());
    Ok(redirect_response_owned(jar, dest))
}

/// Acepta `next` solo si es una ruta interna (empieza con `/` y no `//` ni
/// `/\`), para evitar open redirect a otro dominio.
fn sanitize_next(n: Option<&str>) -> Option<String> {
    let n = n?.trim();
    if !n.starts_with('/') { return None; }
    if n.starts_with("//") || n.starts_with("/\\") { return None; }
    if n.len() > 500 { return None; }
    Some(n.to_string())
}

// ---------- signup ----------

async fn signup_form(
    State(state): State<AppState>,
    crate::auth::MaybeUser(user): crate::auth::MaybeUser,
) -> AppResult<impl IntoResponse> {
    Ok(SignupTemplate {
        year: current_year(),
        google_enabled: state.cfg.google_oauth_enabled(),
        account_role: user.as_ref().map(|u| u.role.clone()).unwrap_or_default(),
        handle: user.map(|u| u.handle).unwrap_or_default(),
    })
}

#[derive(Debug, Deserialize, Validate)]
struct SignupForm {
    #[validate(email(message = "email inválido"))]
    email: String,
    #[validate(
        length(min = 3, max = 32, message = "handle debe tener 3-32 caracteres"),
        regex(path = *HANDLE_RE, message = "handle solo permite letras, números y _")
    )]
    handle: String,
    #[validate(length(min = 10, message = "contraseña mínimo 10 caracteres"))]
    password: String,
    /// "researcher" | "company". Cualquier otro valor cae a researcher.
    #[serde(default)]
    account_type: Option<String>,
}

static HANDLE_RE: once_cell::sync::Lazy<regex::Regex> =
    once_cell::sync::Lazy::new(|| regex::Regex::new(r"^[a-zA-Z0-9_]+$").expect("regex válido"));

async fn signup_submit(
    State(state): State<AppState>,
    jar: SignedCookieJar,
    ConnectInfo(remote): ConnectInfo<SocketAddr>,
    _headers_in: HeaderMap,
    Form(form): Form<SignupForm>,
) -> AppResult<axum::response::Response> {
    if let Err(errs) = form.validate() {
        let msg = first_validation_message(&errs).unwrap_or_else(|| "datos inválidos".into());
        return Ok(error_fragment(&msg));
    }

    let pwhash = auth::hash_password(&form.password)
        .map_err(|e| anyhow::anyhow!("hash password: {e}"))?;

    // Solo dos roles elegibles en el registro. Cualquier otra cosa → researcher.
    let role = match form.account_type.as_deref() {
        Some("company") => "company",
        _ => "researcher",
    };

    let created = db::users::create(
        &state.db,
        db::users::NewUser {
            email: &form.email,
            handle: &form.handle,
            password_hash: &pwhash,
            role,
        },
    )
    .await;

    let user = match created {
        Ok(u) => u,
        Err(sqlx::Error::Database(e)) if e.is_unique_violation() => {
            // No filtramos cuál campo colisionó.
            return Ok(error_fragment("email o handle ya en uso"));
        }
        Err(e) => return Err(e.into()),
    };

    audit::log(&state.db, audit::AuditEntry::new(audit::USER_SIGNUP)
        .actor(user.id).ip(remote.ip())
        .target("user", user.id)
        .metadata(serde_json::json!({ "handle": user.handle }))).await;

    // Generar token de verificación (32 bytes aleatorios, almacenamos el hash)
    let token = {
        let mut bytes = [0u8; 32];
        rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut bytes);
        hex::encode(bytes)
    };
    let token_hash = hex::encode(sha2::Sha256::digest(token.as_bytes()));

    db::email_verifications::create(&state.db, user.id, &token_hash)
        .await
        .map_err(|e| anyhow::anyhow!("email_verif insert: {e}"))?;

    let verify_url = format!("{}/verify-email?token={token}", state.cfg.public_url);
    let _ = state.email.send(&crate::email::Email {
        to: form.email.clone(),
        subject: "Confirma tu cuenta — Escudo Digital".to_string(),
        html_body: format!(
            "<p>Haz clic para activar tu cuenta:</p>\
             <p><a href=\"{verify_url}\">{verify_url}</a></p>"
        ),
        text_body: format!("Activa tu cuenta: {verify_url}"),
    }).await;

    Ok(redirect_response(jar, "/signup/pending"))
}

// ---------- email verification ----------

async fn signup_pending() -> impl IntoResponse {
    SignupPendingTemplate {
        year: current_year(),
        handle: String::new(),
        account_role: String::new(),
    }
}

async fn verify_email(
    State(state): State<AppState>,
    jar: SignedCookieJar,
    ConnectInfo(remote): ConnectInfo<SocketAddr>,
    headers_in: HeaderMap,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> AppResult<axum::response::Response> {
    let token = params
        .get("token")
        .ok_or_else(|| crate::error::AppError::Validation("token requerido".into()))?;
    let token_hash = hex::encode(sha2::Sha256::digest(token.as_bytes()));

    let row = db::email_verifications::consume(&state.db, &token_hash)
        .await
        .map_err(|e| anyhow::anyhow!("email_verif consume: {e}"))?
        .ok_or(crate::error::AppError::NotFound)?;

    sqlx::query("UPDATE users SET status = 'active' WHERE id = $1")
        .bind(row.user_id)
        .execute(&state.db)
        .await
        .map_err(|e| anyhow::anyhow!("update user status: {e}"))?;

    let cookie = issue_session(&state, &row.user_id, &remote, &headers_in).await?;
    let jar = jar.add(cookie);
    let mut headers = HeaderMap::new();
    headers.insert(
        axum::http::header::LOCATION,
        HeaderValue::from_static("/dashboard"),
    );
    Ok((StatusCode::SEE_OTHER, headers, jar, "").into_response())
}

// ---------- logout ----------

async fn logout(
    State(state): State<AppState>,
    jar: SignedCookieJar,
) -> AppResult<axum::response::Response> {
    if let Some(c) = jar.get(SESSION_COOKIE) {
        let token_hash = auth::hash_token(c.value());
        if let Ok(Some(s)) = db::sessions::find_active_by_token_hash(&state.db, &token_hash).await {
            audit::log(&state.db, audit::AuditEntry::new(audit::USER_LOGOUT)
                .actor(s.user_id).target("user", s.user_id)).await;
            let _ = db::sessions::revoke(&state.db, s.id).await;
        }
    }
    let removal = Cookie::build((SESSION_COOKIE, "")).path("/").build();
    let jar = jar.remove(removal);
    // El form de logout es un POST normal (no HTMX), así que devolvemos un
    // redirect HTTP real (303 + Location) que el navegador sigue. Usar el
    // header HX-Redirect aquí dejaría la página en blanco (el navegador no lo
    // interpreta y el cuerpo va vacío).
    let mut headers = HeaderMap::new();
    headers.insert(
        axum::http::header::LOCATION,
        HeaderValue::from_static("/login"),
    );
    Ok((StatusCode::SEE_OTHER, headers, jar, "").into_response())
}

// ---------- helpers ----------

async fn issue_session(
    state: &AppState,
    user_id: &uuid::Uuid,
    remote: &SocketAddr,
    headers_in: &HeaderMap,
) -> AppResult<Cookie<'static>> {
    let token = auth::generate_session_token();
    let token_hash = auth::hash_token(&token);
    let ua = headers_in
        .get(axum::http::header::USER_AGENT)
        .and_then(|v| v.to_str().ok());

    let expires = auth::default_session_expiry();
    db::sessions::create(
        &state.db,
        db::sessions::NewSession {
            user_id: *user_id,
            token_hash: &token_hash,
            ip: Some(remote.ip()),
            user_agent: ua,
            expires_at: expires,
        },
    )
    .await?;
    let _ = db::users::touch_last_login(&state.db, *user_id).await;

    // SIN `.expires()`/max-age a propósito: cookie de **sesión**, el navegador
    // la borra al cerrarse. La sesión en BD sí tiene `expires_at` (cap de 30d)
    // como límite absoluto server-side. (`expires` se usa solo para esa fila.)
    Ok(Cookie::build((SESSION_COOKIE, token))
        .path("/")
        .http_only(true)
        .secure(state.cfg.cookie_secure())
        .same_site(SameSite::Lax)
        .build())
}

fn redirect_response(jar: SignedCookieJar, to: &'static str) -> axum::response::Response {
    let mut headers = HeaderMap::new();
    headers.insert("HX-Redirect", HeaderValue::from_static(to));
    (StatusCode::OK, headers, jar, "").into_response()
}

fn redirect_response_owned(jar: SignedCookieJar, to: String) -> axum::response::Response {
    let mut headers = HeaderMap::new();
    if let Ok(v) = HeaderValue::from_str(&to) {
        headers.insert("HX-Redirect", v);
    }
    (StatusCode::OK, headers, jar, "").into_response()
}

fn error_fragment(msg: &str) -> axum::response::Response {
    use askama::Template;
    let body = FormErrorPartial { message: msg.into() }
        .render()
        .unwrap_or_else(|_| String::from("<div class=\"alert alert-error\">error</div>"));
    axum::response::Html(body).into_response()
}

fn first_validation_message(errors: &validator::ValidationErrors) -> Option<String> {
    errors
        .field_errors()
        .values()
        .flat_map(|v| v.iter())
        .find_map(|e| e.message.as_ref().map(|m| m.to_string()))
}

fn current_year() -> i32 {
    OffsetDateTime::now_utc().year()
}
