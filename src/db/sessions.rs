//! Queries sobre `user_sessions`.

use sqlx::PgPool;
use std::net::IpAddr;
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SessionRecord {
    pub id: Uuid,
    pub user_id: Uuid,
    pub created_at: OffsetDateTime,
    pub expires_at: OffsetDateTime,
}

pub struct NewSession<'a> {
    pub user_id: Uuid,
    pub token_hash: &'a [u8],
    pub ip: Option<IpAddr>,
    pub user_agent: Option<&'a str>,
    pub expires_at: OffsetDateTime,
}

pub async fn create(pool: &PgPool, s: NewSession<'_>) -> Result<SessionRecord, sqlx::Error> {
    let id = Uuid::new_v4();
    sqlx::query_as::<_, SessionRecord>(
        "INSERT INTO user_sessions (id, user_id, token_hash, ip_inet, user_agent, expires_at) \
         VALUES ($1, $2, $3, $4, $5, $6) \
         RETURNING id, user_id, created_at, expires_at",
    )
    .bind(id)
    .bind(s.user_id)
    .bind(s.token_hash)
    .bind(s.ip)
    .bind(s.user_agent)
    .bind(s.expires_at)
    .fetch_one(pool)
    .await
}

/// Sesión válida: existe, no expirada, no revocada.
pub async fn find_active_by_token_hash(
    pool: &PgPool,
    token_hash: &[u8],
) -> Result<Option<SessionRecord>, sqlx::Error> {
    sqlx::query_as::<_, SessionRecord>(
        "SELECT id, user_id, created_at, expires_at \
         FROM user_sessions \
         WHERE token_hash = $1 AND revoked_at IS NULL AND expires_at > now()",
    )
    .bind(token_hash)
    .fetch_optional(pool)
    .await
}

pub async fn revoke(pool: &PgPool, id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE user_sessions SET revoked_at = now() WHERE id = $1 AND revoked_at IS NULL")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}
