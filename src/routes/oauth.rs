//! Sign-In con Google. Standard OAuth 2.0 + OpenID Connect.
//!
//! Flow:
//! 1. `GET /auth/google` → genera `state` aleatorio + PKCE verifier, los
//!    guarda en cookies firmadas de 5min, redirige al consent screen.
//! 2. Google llama `GET /auth/google/callback?code=...&state=...`.
//! 3. Validamos `state`, intercambiamos `code` por tokens, leemos `id_token`
//!    (JWT con `email_verified` y `sub`), buscamos o creamos el user, abrimos
//!    sesión local y redirigimos a `next` (o /dashboard).
//!
//! Si `GOOGLE_CLIENT_ID` no está en env, las rutas devuelven 404 (no se
//! exponen) y el botón "Continuar con Google" en login/signup no aparece.

use axum::extract::{ConnectInfo, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::get;
use axum::Router;
use axum_extra::extract::cookie::{Cookie, SameSite, SignedCookieJar};
use oauth2::basic::BasicClient;
use oauth2::reqwest::async_http_client;
use oauth2::{
    AuthUrl, AuthorizationCode, ClientId, ClientSecret, CsrfToken, PkceCodeChallenge,
    PkceCodeVerifier, RedirectUrl, Scope, TokenResponse, TokenUrl,
};
use secrecy::ExposeSecret;
use serde::Deserialize;
use std::net::SocketAddr;

use crate::audit;
use crate::auth::{self, SESSION_COOKIE};
use crate::db;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

const STATE_COOKIE: &str = "bb_oauth_state";
const PKCE_COOKIE: &str = "bb_oauth_pkce";
const NEXT_COOKIE: &str = "bb_oauth_next";
const PROVIDER_GOOGLE: &str = "google";

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/auth/google", get(start))
        .route("/auth/google/callback", get(callback))
}

fn google_client(state: &AppState) -> AppResult<BasicClient> {
    if !state.cfg.google_oauth_enabled() {
        return Err(AppError::NotFound);
    }
    let cid = ClientId::new(state.cfg.google_client_id.clone());
    let csec = ClientSecret::new(state.cfg.google_client_secret.expose_secret().to_string());
    let auth_url = AuthUrl::new("https://accounts.google.com/o/oauth2/v2/auth".into())
        .map_err(|e| anyhow::anyhow!("auth_url: {e}"))?;
    let token_url = TokenUrl::new("https://oauth2.googleapis.com/token".into())
        .map_err(|e| anyhow::anyhow!("token_url: {e}"))?;
    let redirect = RedirectUrl::new(state.cfg.google_redirect_uri())
        .map_err(|e| anyhow::anyhow!("redirect_url: {e}"))?;
    Ok(BasicClient::new(cid, Some(csec), auth_url, Some(token_url)).set_redirect_uri(redirect))
}

// ----------------------------------------------------------------------------
// /auth/google — arranque del flow
// ----------------------------------------------------------------------------

#[derive(Deserialize, Default)]
struct StartQuery {
    #[serde(default)]
    next: Option<String>,
}

async fn start(
    State(state): State<AppState>,
    jar: SignedCookieJar,
    Query(q): Query<StartQuery>,
) -> AppResult<Response> {
    let client = google_client(&state)?;
    let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();

    let (auth_url, csrf_token) = client
        .authorize_url(CsrfToken::new_random)
        // Scopes mínimos: identidad. No pedimos drive/contacts/etc.
        .add_scope(Scope::new("openid".into()))
        .add_scope(Scope::new("email".into()))
        .add_scope(Scope::new("profile".into()))
        .set_pkce_challenge(pkce_challenge)
        .url();

    // Guardamos state + pkce verifier + next en cookies firmadas de 5min.
    let secure = state.cfg.cookie_secure();
    let make = |name: &'static str, value: String| {
        Cookie::build((name, value))
            .path("/auth/google")
            .http_only(true)
            .secure(secure)
            .same_site(SameSite::Lax)
            .max_age(time::Duration::minutes(5))
            .build()
    };
    let next = sanitize_next(q.next.as_deref()).unwrap_or_default();
    let jar = jar
        .add(make(STATE_COOKIE, csrf_token.secret().to_string()))
        .add(make(PKCE_COOKIE, pkce_verifier.secret().to_string()))
        .add(make(NEXT_COOKIE, next));

    Ok((jar, Redirect::to(auth_url.as_str())).into_response())
}

// ----------------------------------------------------------------------------
// /auth/google/callback — Google trae code+state
// ----------------------------------------------------------------------------

#[derive(Deserialize)]
struct CallbackQuery {
    #[serde(default)]
    code: Option<String>,
    #[serde(default)]
    state: Option<String>,
    #[serde(default)]
    error: Option<String>,
}

async fn callback(
    State(state): State<AppState>,
    jar: SignedCookieJar,
    ConnectInfo(remote): ConnectInfo<SocketAddr>,
    headers_in: HeaderMap,
    Query(q): Query<CallbackQuery>,
) -> AppResult<Response> {
    // Si el user canceló en la pantalla de consent, Google devuelve ?error=...
    if let Some(err) = q.error {
        tracing::warn!(error = %err, "google oauth: user canceled or error");
        return Ok(consume_cookies_and_redirect(jar, "/login?oauth_error=1"));
    }

    let client = google_client(&state)?;

    // Validar state: el que llegó por query debe coincidir con la cookie.
    let cookie_state = jar
        .get(STATE_COOKIE)
        .map(|c| c.value().to_string())
        .ok_or(AppError::Validation("oauth: state cookie ausente".into()))?;
    let query_state = q.state.unwrap_or_default();
    if cookie_state != query_state || query_state.is_empty() {
        return Err(AppError::Validation("oauth: state inválido".into()));
    }

    let code = q.code.ok_or_else(|| AppError::Validation("oauth: code ausente".into()))?;
    let pkce_verifier = jar
        .get(PKCE_COOKIE)
        .map(|c| PkceCodeVerifier::new(c.value().to_string()))
        .ok_or(AppError::Validation("oauth: pkce ausente".into()))?;
    let next_dest = jar.get(NEXT_COOKIE).map(|c| c.value().to_string()).unwrap_or_default();

    // Intercambiar code por token.
    let token = client
        .exchange_code(AuthorizationCode::new(code))
        .set_pkce_verifier(pkce_verifier)
        .request_async(async_http_client)
        .await
        .map_err(|e| anyhow::anyhow!("oauth exchange: {e}"))?;

    let access_token = token.access_token().secret();
    let profile = fetch_google_profile(access_token).await?;

    if !profile.email_verified {
        return Err(AppError::Validation(
            "tu email de Google no está verificado todavía".into(),
        ));
    }

    // Buscar por (provider, subject) → si existe, login directo.
    // Si no, buscar por email → si coincide, vincular (el dueño del email es el mismo, Google lo verificó).
    // Si nada, crear cuenta nueva.
    let provider = PROVIDER_GOOGLE;
    let existing = db::users::find_by_oauth(&state.db, provider, &profile.sub).await?;
    let user = if let Some(u) = existing {
        u
    } else if let Some(by_email) = db::users::find_by_email(&state.db, &profile.email).await? {
        // Vincular: el email coincide y Google ya lo verificó → es el mismo dueño.
        db::users::link_oauth(&state.db, by_email.id, provider, &profile.sub).await?;
        by_email
    } else {
        // Crear cuenta nueva. Handle derivado del email; si choca, append número.
        let base = derive_handle(&profile.email);
        let handle = unique_handle(&state.db, &base).await?;
        let dummy = auth::hash_password(&random_password()).map_err(|e| anyhow::anyhow!("dummy hash: {e}"))?;
        let created = db::users::create_oauth(
            &state.db,
            db::users::NewOauthUser {
                email: &profile.email,
                handle: &handle,
                password_hash: &dummy,
                provider,
                subject: &profile.sub,
                display_name: profile.name.as_deref(),
            },
        )
        .await
        .map_err(|e| anyhow::anyhow!("create oauth user: {e}"))?;
        audit::log(&state.db, audit::AuditEntry::new(audit::USER_SIGNUP)
            .actor(created.id).ip(remote.ip())
            .target("user", created.id)
            .metadata(serde_json::json!({ "via": "google_oauth", "handle": created.handle })))
            .await;
        created
    };

    if user.status != "active" {
        return Err(AppError::Forbidden);
    }

    // Abrir sesión local (mismo flujo que login normal).
    let cookie = auth::generate_session_token();
    let token_hash = auth::hash_token(&cookie);
    let ua = headers_in
        .get(axum::http::header::USER_AGENT)
        .and_then(|v| v.to_str().ok());
    let expires = auth::default_session_expiry();
    db::sessions::create(
        &state.db,
        db::sessions::NewSession {
            user_id: user.id,
            token_hash: &token_hash,
            ip: Some(remote.ip()),
            user_agent: ua,
            expires_at: expires,
        },
    )
    .await?;
    let _ = db::users::touch_last_login(&state.db, user.id).await;
    audit::log(&state.db, audit::AuditEntry::new(audit::USER_LOGIN)
        .actor(user.id).ip(remote.ip()).target("user", user.id)
        .metadata(serde_json::json!({ "via": "google_oauth" })))
        .await;

    let session_cookie = Cookie::build((SESSION_COOKIE, cookie))
        .path("/")
        .http_only(true)
        .secure(state.cfg.cookie_secure())
        .same_site(SameSite::Lax)
        .expires(expires)
        .build();
    let jar = jar.add(session_cookie);

    let dest = if next_dest.is_empty() { "/dashboard".to_string() } else { next_dest };
    Ok(consume_cookies_and_redirect(jar, &dest))
}

// ----------------------------------------------------------------------------
// helpers
// ----------------------------------------------------------------------------

/// Borra las cookies de OAuth (state/pkce/next) y emite un redirect HTTP 303.
fn consume_cookies_and_redirect(jar: SignedCookieJar, to: &str) -> Response {
    let remove = |n: &'static str| Cookie::build((n, "")).path("/auth/google").build();
    let jar = jar
        .remove(remove(STATE_COOKIE))
        .remove(remove(PKCE_COOKIE))
        .remove(remove(NEXT_COOKIE));
    let mut headers = HeaderMap::new();
    if let Ok(v) = axum::http::HeaderValue::from_str(to) {
        headers.insert(axum::http::header::LOCATION, v);
    }
    (StatusCode::SEE_OTHER, headers, jar, "").into_response()
}

#[derive(Debug, Deserialize)]
struct GoogleProfile {
    sub: String,
    email: String,
    #[serde(default)]
    email_verified: bool,
    #[serde(default)]
    name: Option<String>,
}

async fn fetch_google_profile(access_token: &str) -> AppResult<GoogleProfile> {
    let resp = reqwest::Client::new()
        .get("https://openidconnect.googleapis.com/v1/userinfo")
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("userinfo: {e}"))?;
    if !resp.status().is_success() {
        return Err(anyhow::anyhow!("userinfo status {}", resp.status()).into());
    }
    let p: GoogleProfile = resp
        .json()
        .await
        .map_err(|e| anyhow::anyhow!("userinfo json: {e}"))?;
    Ok(p)
}

/// Convierte un email en un handle candidato: parte local saneada.
fn derive_handle(email: &str) -> String {
    let local = email.split('@').next().unwrap_or("user");
    let cleaned: String = local
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
        .take(28)
        .collect();
    if cleaned.len() < 3 { format!("user_{cleaned}") } else { cleaned }
}

/// Si el handle base ya existe, prueba `base2`, `base3`, ... hasta encontrar uno libre.
async fn unique_handle(pool: &sqlx::PgPool, base: &str) -> AppResult<String> {
    let mut candidate = base.to_string();
    let mut n: u32 = 2;
    loop {
        let row: Option<(uuid::Uuid,)> =
            sqlx::query_as("SELECT id FROM users WHERE handle = $1")
                .bind(&candidate)
                .fetch_optional(pool)
                .await?;
        if row.is_none() {
            return Ok(candidate);
        }
        candidate = format!("{base}{n}");
        n += 1;
        if n > 9999 {
            return Err(AppError::Internal(anyhow::anyhow!("no free handle")));
        }
    }
}

fn random_password() -> String {
    use rand::RngCore;
    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    hex::encode(bytes)
}

fn sanitize_next(n: Option<&str>) -> Option<String> {
    let n = n?.trim();
    if !n.starts_with('/') { return None; }
    if n.starts_with("//") || n.starts_with("/\\") { return None; }
    if n.len() > 500 { return None; }
    Some(n.to_string())
}
