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

use crate::audit;
use crate::auth::{self, SESSION_COOKIE};
use crate::db;
use crate::error::AppResult;
use crate::state::AppState;
use crate::web::templates::{FormErrorPartial, LoginTemplate, SignupTemplate};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/login", get(login_form).post(login_submit))
        .route("/signup", get(signup_form).post(signup_submit))
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
    axum::extract::Query(q): axum::extract::Query<LoginQuery>,
) -> AppResult<impl IntoResponse> {
    let next = sanitize_next(q.next.as_deref()).unwrap_or_default();
    Ok(LoginTemplate {
        year: current_year(),
        next,
        google_enabled: state.cfg.google_oauth_enabled(),
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

async fn signup_form(State(state): State<AppState>) -> AppResult<impl IntoResponse> {
    Ok(SignupTemplate {
        year: current_year(),
        google_enabled: state.cfg.google_oauth_enabled(),
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
}

static HANDLE_RE: once_cell::sync::Lazy<regex::Regex> =
    once_cell::sync::Lazy::new(|| regex::Regex::new(r"^[a-zA-Z0-9_]+$").expect("regex válido"));

async fn signup_submit(
    State(state): State<AppState>,
    jar: SignedCookieJar,
    ConnectInfo(remote): ConnectInfo<SocketAddr>,
    headers_in: HeaderMap,
    Form(form): Form<SignupForm>,
) -> AppResult<axum::response::Response> {
    if let Err(errs) = form.validate() {
        let msg = first_validation_message(&errs).unwrap_or_else(|| "datos inválidos".into());
        return Ok(error_fragment(&msg));
    }

    let pwhash = auth::hash_password(&form.password)
        .map_err(|e| anyhow::anyhow!("hash password: {e}"))?;

    let created = db::users::create(
        &state.db,
        db::users::NewUser {
            email: &form.email,
            handle: &form.handle,
            password_hash: &pwhash,
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

    let cookie = issue_session(&state, &user.id, &remote, &headers_in).await?;
    audit::log(&state.db, audit::AuditEntry::new(audit::USER_SIGNUP)
        .actor(user.id).ip(remote.ip())
        .target("user", user.id)
        .metadata(serde_json::json!({ "handle": user.handle }))).await;
    let jar = jar.add(cookie);
    Ok(redirect_response(jar, "/dashboard"))
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
    Ok(redirect_response(jar, "/login"))
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

    Ok(Cookie::build((SESSION_COOKIE, token))
        .path("/")
        .http_only(true)
        .secure(state.cfg.cookie_secure())
        .same_site(SameSite::Lax)
        .expires(expires)
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
