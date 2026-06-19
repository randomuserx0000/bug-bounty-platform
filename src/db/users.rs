//! Queries sobre `users`. Solo lectura/escritura cruda, sin negocio.

use sqlx::PgPool;
use time::OffsetDateTime;
use uuid::Uuid;

/// Subset de columnas de `users` que necesita la app en runtime.
/// Mantenemos esto chico — campos pesados (bio, avatar) se cargan aparte.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct UserRecord {
    pub id: Uuid,
    pub email: String,
    pub handle: String,
    pub password_hash: String,
    /// Enum `user_role` mapeado como TEXT por simplicidad inicial.
    pub role: String,
    /// Enum `user_status` mapeado como TEXT.
    pub status: String,
    pub display_name: Option<String>,
    pub created_at: OffsetDateTime,
}

const COLUMNS: &str = "id, email, handle, password_hash, role::text AS role, \
                       status::text AS status, display_name, created_at";

pub async fn find_by_email(pool: &PgPool, email: &str) -> Result<Option<UserRecord>, sqlx::Error> {
    let sql = format!("SELECT {COLUMNS} FROM users WHERE email = $1");
    sqlx::query_as::<_, UserRecord>(&sql)
        .bind(email)
        .fetch_optional(pool)
        .await
}

pub async fn find_by_id(pool: &PgPool, id: Uuid) -> Result<Option<UserRecord>, sqlx::Error> {
    let sql = format!("SELECT {COLUMNS} FROM users WHERE id = $1");
    sqlx::query_as::<_, UserRecord>(&sql)
        .bind(id)
        .fetch_optional(pool)
        .await
}

pub struct NewUser<'a> {
    pub email: &'a str,
    pub handle: &'a str,
    pub password_hash: &'a str,
    /// 'researcher' | 'company'. El caller ya lo valida.
    pub role: &'a str,
}

/// Inserta un usuario nuevo en estado 'pending' (requiere verificación de email).
pub async fn create(pool: &PgPool, u: NewUser<'_>) -> Result<UserRecord, sqlx::Error> {
    let id = Uuid::new_v4();
    let sql = format!(
        "INSERT INTO users (id, email, handle, password_hash, role, status) \
         VALUES ($1, $2, $3, $4, $5::user_role, 'pending') \
         RETURNING {COLUMNS}"
    );
    sqlx::query_as::<_, UserRecord>(&sql)
        .bind(id)
        .bind(u.email)
        .bind(u.handle)
        .bind(u.password_hash)
        .bind(u.role)
        .fetch_one(pool)
        .await
}

/// Busca un user por su identidad OAuth (`provider` + `subject` del provider).
pub async fn find_by_oauth(
    pool: &PgPool,
    provider: &str,
    subject: &str,
) -> Result<Option<UserRecord>, sqlx::Error> {
    let sql = format!(
        "SELECT {COLUMNS} FROM users \
         WHERE oauth_provider = $1 AND oauth_subject = $2"
    );
    sqlx::query_as::<_, UserRecord>(&sql)
        .bind(provider)
        .bind(subject)
        .fetch_optional(pool)
        .await
}

pub struct NewOauthUser<'a> {
    pub email: &'a str,
    pub handle: &'a str,
    pub password_hash: &'a str,    // dummy unguessable; el user nunca lo usa
    pub provider: &'a str,
    pub subject: &'a str,
    pub display_name: Option<&'a str>,
}

/// Crea un user que arranca vinculado a OAuth (sin password real conocido).
pub async fn create_oauth(pool: &PgPool, u: NewOauthUser<'_>) -> Result<UserRecord, sqlx::Error> {
    let id = Uuid::new_v4();
    let sql = format!(
        "INSERT INTO users (id, email, handle, password_hash, role, status, \
                            display_name, oauth_provider, oauth_subject) \
         VALUES ($1, $2, $3, $4, 'researcher', 'active', $5, $6, $7) \
         RETURNING {COLUMNS}"
    );
    sqlx::query_as::<_, UserRecord>(&sql)
        .bind(id)
        .bind(u.email)
        .bind(u.handle)
        .bind(u.password_hash)
        .bind(u.display_name)
        .bind(u.provider)
        .bind(u.subject)
        .fetch_one(pool)
        .await
}

/// Vincula un user existente (mismo email) a un provider OAuth.
pub async fn link_oauth(
    pool: &PgPool,
    user_id: Uuid,
    provider: &str,
    subject: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE users SET oauth_provider = $2, oauth_subject = $3, updated_at = now() \
         WHERE id = $1"
    )
    .bind(user_id)
    .bind(provider)
    .bind(subject)
    .execute(pool)
    .await?;
    Ok(())
}

/// Actualiza solo el handle (nick) del usuario. Falla con
/// `sqlx::Error::Database` si el nuevo handle ya está tomado (unique).
pub async fn update_handle(
    pool: &PgPool,
    id: Uuid,
    new_handle: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE users SET handle = $2, updated_at = now() WHERE id = $1")
        .bind(id)
        .bind(new_handle)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn touch_last_login(pool: &PgPool, id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE users SET last_login_at = now() WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Devuelve un mapa `id → handle` para los UUIDs dados (en una sola query).
pub async fn handles_by_ids(
    pool: &PgPool,
    ids: &[Uuid],
) -> Result<std::collections::HashMap<Uuid, String>, sqlx::Error> {
    if ids.is_empty() {
        return Ok(std::collections::HashMap::new());
    }
    let rows: Vec<(Uuid, String)> = sqlx::query_as(
        "SELECT id, handle FROM users WHERE id = ANY($1)"
    )
    .bind(ids)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().collect())
}
