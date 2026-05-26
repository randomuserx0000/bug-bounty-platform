//! Primitivas de autenticación: passwords, tokens de sesión, extractor.
//!
//! ## Threat model resumido
//!
//! - **Timing oracle por email**: `verify_password_or_dummy` corre argon2
//!   incluso cuando el usuario no existe. El atacante no aprende si el
//!   email está registrado por diferencias de tiempo.
//! - **Filtración de DB**: los tokens viajan en cookie pero se guardan
//!   hasheados (SHA-256) en `user_sessions.token_hash`. Una DB filtrada
//!   no permite secuestrar sesiones activas.
//! - **CSRF / XSS**: cookie firmada + HttpOnly + SameSite=Lax + Secure
//!   (cuando PUBLIC_URL es HTTPS).

use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::SaltString;
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use axum::async_trait;
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::response::IntoResponse;
use axum_extra::extract::SignedCookieJar;
use rand::RngCore;
use sha2::{Digest, Sha256};
use time::{Duration, OffsetDateTime};

use crate::db::sessions::SessionRecord;
use crate::db::users::UserRecord;
use crate::error::AppError;
use crate::state::AppState;

/// Nombre de la cookie de sesión.
pub const SESSION_COOKIE: &str = "bb_session";

/// Vida útil de una sesión.
pub const SESSION_TTL: Duration = Duration::days(30);

/// Hash precomputado para `verify_password_or_dummy`. Se calcula una vez
/// al primer uso y se cachea estáticamente. El password "irrelevante" no
/// importa: solo necesitamos un hash válido para que argon2::verify corra
/// el trabajo CPU completo.
static DUMMY_HASH: std::sync::OnceLock<String> = std::sync::OnceLock::new();

fn dummy_hash() -> &'static str {
    DUMMY_HASH.get_or_init(|| {
        let salt = SaltString::generate(&mut OsRng);
        Argon2::default()
            .hash_password(b"dummy-password-not-used", &salt)
            .map(|h| h.to_string())
            .unwrap_or_else(|_| String::new())
    })
}

/// Hashea un password para guardar en `users.password_hash`.
pub fn hash_password(password: &str) -> Result<String, argon2::password_hash::Error> {
    let salt = SaltString::generate(&mut OsRng);
    let hash = Argon2::default().hash_password(password.as_bytes(), &salt)?;
    Ok(hash.to_string())
}

/// Verifica un password contra el hash de un usuario, **o** corre el
/// trabajo CPU contra un hash dummy si el usuario no existe.
///
/// Esto cierra el timing oracle: el atacante no puede distinguir
/// "email no existe" de "email existe pero password incorrecto" por
/// tiempos de respuesta.
pub fn verify_password_or_dummy(stored: Option<&str>, attempted: &str) -> bool {
    let dummy = dummy_hash();
    let hash_str: &str = stored.unwrap_or(dummy);
    let Ok(parsed) = PasswordHash::new(hash_str) else {
        return false;
    };
    let verified = Argon2::default()
        .verify_password(attempted.as_bytes(), &parsed)
        .is_ok();
    verified && stored.is_some()
}

/// Genera un token de sesión: 32 bytes aleatorios codificados en hex (64 chars).
pub fn generate_session_token() -> String {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    hex::encode(bytes)
}

/// Hashea un token de sesión con SHA-256. Lo que se guarda en DB.
pub fn hash_token(token: &str) -> Vec<u8> {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    hasher.finalize().to_vec()
}

/// Calcula la fecha de expiración estándar de una sesión nueva.
pub fn default_session_expiry() -> OffsetDateTime {
    OffsetDateTime::now_utc() + SESSION_TTL
}

/// Extractor que carga el usuario actual desde la cookie de sesión.
///
/// Uso en handlers:
/// ```ignore
/// async fn dashboard(user: CurrentUser) -> ... { ... }
/// ```
///
/// Si no hay sesión válida:
/// - **Navegación normal del browser** → 303 redirect a `/login?next=<path>`.
///   El usuario aterriza en login y al entrar vuelve a donde quería ir.
/// - **Request HTMX** (header `HX-Request: true`) → 200 + `HX-Redirect` header,
///   que HTMX intercepta para navegar el browser sin mostrar el 401 plano.
/// - **Usuario suspendido/banned** (status != 'active') → 403 Forbidden con
///   mensaje (no redirige, porque no tiene sentido mandarlo a login otra vez).
pub struct CurrentUser {
    pub user: UserRecord,
    pub session: SessionRecord,
}

#[async_trait]
impl FromRequestParts<AppState> for CurrentUser {
    type Rejection = axum::response::Response;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let jar = SignedCookieJar::<axum_extra::extract::cookie::Key>::from_headers(
            &parts.headers,
            state.cookie_key.clone(),
        );
        let path = parts.uri.path().to_string();
        let has_raw_cookie = parts
            .headers
            .get(axum::http::header::COOKIE)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.contains(SESSION_COOKIE))
            .unwrap_or(false);

        let token = match jar.get(SESSION_COOKIE) {
            Some(t) => t,
            None => {
                tracing::info!(
                    path = %path,
                    has_raw_cookie,
                    "auth: no signed session cookie (jar miss)"
                );
                return Err(login_redirect(parts));
            }
        };
        let token_value = token.value().to_string();
        let token_hash = hash_token(&token_value);

        let session = match crate::db::sessions::find_active_by_token_hash(&state.db, &token_hash).await {
            Ok(Some(s)) => s,
            Ok(None) => {
                tracing::info!(
                    path = %path,
                    "auth: cookie present but no active session row"
                );
                return Err(login_redirect(parts));
            }
            Err(e) => return Err(AppError::Db(e).into_response()),
        };

        let user = match crate::db::users::find_by_id(&state.db, session.user_id).await {
            Ok(Some(u)) => u,
            Ok(None) => return Err(login_redirect(parts)),
            Err(e) => return Err(AppError::Db(e).into_response()),
        };

        if user.status != "active" {
            return Err(AppError::Forbidden.into_response());
        }

        Ok(CurrentUser { user, session })
    }
}

/// Construye la respuesta de "no autenticado": HX-Redirect si vino por HTMX,
/// 303 Location a `/login?next=<original_path>` si fue navegación directa.
fn login_redirect(parts: &Parts) -> axum::response::Response {
    use axum::http::{HeaderValue, StatusCode};

    // Path original con query, para que el login pueda volver al lugar correcto.
    let path_q = parts.uri.path_and_query().map(|p| p.as_str()).unwrap_or("/");
    let next_encoded = urlencoding::encode(path_q);
    let target = format!("/login?next={next_encoded}");

    let is_htmx = parts
        .headers
        .get("HX-Request")
        .and_then(|v| v.to_str().ok())
        == Some("true");

    if is_htmx {
        let mut resp = axum::response::Response::new(axum::body::Body::empty());
        *resp.status_mut() = StatusCode::OK;
        if let Ok(v) = HeaderValue::from_str(&target) {
            resp.headers_mut().insert("HX-Redirect", v);
        }
        resp
    } else {
        axum::response::Redirect::to(&target).into_response()
    }
}
