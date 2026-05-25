//! Queries de `payment_methods`.
//!
//! Las funciones aquí trabajan con el blob ya cifrado: el cifrado/descifrado
//! sucede en la capa de handlers, que es quien tiene la key del AppState.
//! Así esta capa no toca crypto y se puede testear con bytes arbitrarios.

use sqlx::PgPool;
use time::OffsetDateTime;

use crate::domain::ids::{PaymentMethodId, UserId};
use crate::payments::PaymentRail;

#[derive(Debug, sqlx::FromRow)]
pub struct PaymentMethodRow {
    pub id: PaymentMethodId,
    pub user_id: UserId,
    pub rail: PaymentRail,
    pub label: Option<String>,
    pub details_enc: Vec<u8>,
    pub is_default: bool,
    pub verified_at: Option<OffsetDateTime>,
    pub created_at: OffsetDateTime,
}

pub async fn list_for_user(
    pool: &PgPool,
    user_id: UserId,
) -> Result<Vec<PaymentMethodRow>, sqlx::Error> {
    sqlx::query_as::<_, PaymentMethodRow>(
        r#"
        SELECT id, user_id, rail, label, details_enc, is_default, verified_at, created_at
        FROM payment_methods
        WHERE user_id = $1
        ORDER BY is_default DESC, created_at DESC
        "#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
}

/// Devuelve el método de pago marcado como default del user, si existe.
/// Si no, devuelve el más reciente (fallback razonable para users con un
/// solo método sin haber marcado default).
pub async fn find_default_for_user(
    pool: &PgPool,
    user_id: UserId,
) -> Result<Option<PaymentMethodRow>, sqlx::Error> {
    sqlx::query_as::<_, PaymentMethodRow>(
        r#"
        SELECT id, user_id, rail, label, details_enc, is_default, verified_at, created_at
        FROM payment_methods
        WHERE user_id = $1
        ORDER BY is_default DESC, created_at DESC
        LIMIT 1
        "#,
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
}

pub async fn find_by_id(
    pool: &PgPool,
    id: PaymentMethodId,
) -> Result<Option<PaymentMethodRow>, sqlx::Error> {
    sqlx::query_as::<_, PaymentMethodRow>(
        r#"
        SELECT id, user_id, rail, label, details_enc, is_default, verified_at, created_at
        FROM payment_methods
        WHERE id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await
}

pub struct NewPaymentMethod<'a> {
    pub user_id: UserId,
    pub rail: PaymentRail,
    pub label: Option<&'a str>,
    pub details_enc: &'a [u8],
    pub is_default: bool,
}

pub async fn create(pool: &PgPool, m: NewPaymentMethod<'_>) -> Result<PaymentMethodId, sqlx::Error> {
    let id = PaymentMethodId::new();
    // Si se marca como default, primero limpio el flag de los demás del usuario
    // dentro de la misma transacción para no violar `uq_pm_user_default`.
    let mut tx = pool.begin().await?;
    if m.is_default {
        sqlx::query("UPDATE payment_methods SET is_default = FALSE WHERE user_id = $1")
            .bind(m.user_id)
            .execute(&mut *tx)
            .await?;
    }
    sqlx::query(
        r#"
        INSERT INTO payment_methods
            (id, user_id, rail, label, details_enc, is_default)
        VALUES ($1, $2, $3, $4, $5, $6)
        "#,
    )
    .bind(id)
    .bind(m.user_id)
    .bind(m.rail)
    .bind(m.label)
    .bind(m.details_enc)
    .bind(m.is_default)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(id)
}

pub async fn delete(
    pool: &PgPool,
    id: PaymentMethodId,
    user_id: UserId,
) -> Result<u64, sqlx::Error> {
    let res = sqlx::query("DELETE FROM payment_methods WHERE id = $1 AND user_id = $2")
        .bind(id)
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(res.rows_affected())
}

pub async fn set_default(
    pool: &PgPool,
    id: PaymentMethodId,
    user_id: UserId,
) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::query("UPDATE payment_methods SET is_default = FALSE WHERE user_id = $1")
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE payment_methods SET is_default = TRUE WHERE id = $1 AND user_id = $2")
        .bind(id)
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}
