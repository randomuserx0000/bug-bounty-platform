use sqlx::PgPool;
use uuid::Uuid;

pub async fn create(pool: &PgPool, user_id: Uuid, token_hash: &str) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO email_verifications (id, user_id, token_hash) VALUES ($1, $2, $3)",
    )
    .bind(Uuid::new_v4())
    .bind(user_id)
    .bind(token_hash)
    .execute(pool)
    .await?;
    Ok(())
}

pub struct VerificationRow {
    pub user_id: Uuid,
}

pub async fn consume(pool: &PgPool, token_hash: &str) -> sqlx::Result<Option<VerificationRow>> {
    let row = sqlx::query_as::<_, (Uuid,)>(
        "DELETE FROM email_verifications
         WHERE token_hash = $1 AND expires_at > now()
         RETURNING user_id",
    )
    .bind(token_hash)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|(user_id,)| VerificationRow { user_id }))
}
