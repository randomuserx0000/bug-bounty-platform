//! Queries y operaciones transaccionales sobre `payouts`.
//!
//! La pieza no-trivial es `create_pending_with_debit`: crear el payout
//! `pending` y debitar `companies.escrow_balance_cents` en la misma
//! transacción. Si el escrow no alcanza, Postgres rompe el CHECK
//! `escrow_balance_cents >= 0` y devolvemos un error específico — el
//! caller decide caer a `failed` con error_message.

use sqlx::PgPool;
use time::OffsetDateTime;

use crate::db::companies::adjust_escrow_in_tx;
use crate::domain::ids::{CompanyId, PaymentMethodId, PayoutId, ReportId, UserId};
use crate::domain::payout::{PayoutRecord, PayoutStatus};
use crate::payments::PaymentRail;

const COLUMNS: &str = "id, report_id, company_id, user_id, payment_method_id, rail, \
                       amount_cents, fee_cents, status, tx_ref, error_message, \
                       created_at, sent_at";

pub async fn find_by_id(
    pool: &PgPool,
    id: PayoutId,
) -> Result<Option<PayoutRecord>, sqlx::Error> {
    let sql = format!("SELECT {COLUMNS} FROM payouts WHERE id = $1");
    sqlx::query_as::<_, PayoutRecord>(&sql)
        .bind(id)
        .fetch_optional(pool)
        .await
}

pub async fn find_for_report(
    pool: &PgPool,
    report_id: ReportId,
) -> Result<Option<PayoutRecord>, sqlx::Error> {
    let sql = format!(
        "SELECT {COLUMNS} FROM payouts WHERE report_id = $1 \
         ORDER BY created_at DESC LIMIT 1"
    );
    sqlx::query_as::<_, PayoutRecord>(&sql)
        .bind(report_id)
        .fetch_optional(pool)
        .await
}

pub async fn list_for_company(
    pool: &PgPool,
    company_id: CompanyId,
) -> Result<Vec<PayoutRecord>, sqlx::Error> {
    let sql = format!(
        "SELECT {COLUMNS} FROM payouts WHERE company_id = $1 \
         ORDER BY \
            CASE status WHEN 'pending' THEN 0 WHEN 'processing' THEN 1 \
                        WHEN 'failed' THEN 2 WHEN 'sent' THEN 3 ELSE 4 END, \
            created_at DESC"
    );
    sqlx::query_as::<_, PayoutRecord>(&sql)
        .bind(company_id)
        .fetch_all(pool)
        .await
}

pub async fn list_for_reporter(
    pool: &PgPool,
    user_id: UserId,
) -> Result<Vec<PayoutRecord>, sqlx::Error> {
    let sql = format!(
        "SELECT {COLUMNS} FROM payouts WHERE user_id = $1 ORDER BY created_at DESC"
    );
    sqlx::query_as::<_, PayoutRecord>(&sql)
        .bind(user_id)
        .fetch_all(pool)
        .await
}

pub struct NewPayoutInput {
    pub report_id: ReportId,
    pub company_id: CompanyId,
    pub user_id: UserId,
    pub payment_method_id: Option<PaymentMethodId>,
    pub rail: PaymentRail,
    pub amount_cents: i64,
}

/// Crea un payout en estado `pending` y debita el escrow de la company en
/// la misma transacción. Si el escrow queda negativo, el CHECK rompe la
/// transacción y devolvemos error — el caller decide caer a `failed`.
pub async fn create_pending_with_debit(
    pool: &PgPool,
    n: NewPayoutInput,
) -> Result<PayoutId, sqlx::Error> {
    let id = PayoutId::new();
    let mut tx = pool.begin().await?;
    adjust_escrow_in_tx(&mut tx, n.company_id, -n.amount_cents).await?;
    sqlx::query(
        "INSERT INTO payouts
            (id, report_id, company_id, user_id, payment_method_id, rail,
             amount_cents, status)
         VALUES ($1, $2, $3, $4, $5, $6, $7, 'pending')",
    )
    .bind(id)
    .bind(n.report_id)
    .bind(n.company_id)
    .bind(n.user_id)
    .bind(n.payment_method_id)
    .bind(n.rail)
    .bind(n.amount_cents)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(id)
}

/// Crea un payout en estado `failed` (no toca escrow). Para casos donde
/// no se puede ni intentar: reporter sin método de pago, escrow insuficiente.
pub async fn create_failed(
    pool: &PgPool,
    n: NewPayoutInput,
    error_message: &str,
) -> Result<PayoutId, sqlx::Error> {
    let id = PayoutId::new();
    sqlx::query(
        "INSERT INTO payouts
            (id, report_id, company_id, user_id, payment_method_id, rail,
             amount_cents, status, error_message)
         VALUES ($1, $2, $3, $4, $5, $6, $7, 'failed', $8)",
    )
    .bind(id)
    .bind(n.report_id)
    .bind(n.company_id)
    .bind(n.user_id)
    .bind(n.payment_method_id)
    .bind(n.rail)
    .bind(n.amount_cents)
    .bind(error_message)
    .execute(pool)
    .await?;
    Ok(id)
}

/// Marca como `sent` con la referencia (txid bancaria/blockchain). NO toca
/// escrow porque ya se debitó al pasar a `pending`.
pub async fn mark_sent(
    pool: &PgPool,
    id: PayoutId,
    tx_ref: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE payouts SET status = 'sent', tx_ref = $2, sent_at = now() \
         WHERE id = $1 AND status IN ('pending', 'processing')",
    )
    .bind(id)
    .bind(tx_ref)
    .execute(pool)
    .await?;
    Ok(())
}

/// Marca como `failed` desde `pending`/`processing` y REEMBOLSA el escrow
/// que se debitó al crear. Para el caso en que el admin descubre que el
/// payout es inviable (ej: cuenta bancaria cerrada).
pub async fn mark_failed_and_refund(
    pool: &PgPool,
    id: PayoutId,
    error_message: &str,
) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    let row: Option<(CompanyId, i64, PayoutStatus)> = sqlx::query_as(
        "SELECT company_id, amount_cents, status FROM payouts WHERE id = $1 FOR UPDATE",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?;

    let Some((company_id, amount, status)) = row else {
        return Err(sqlx::Error::RowNotFound);
    };
    // Solo reembolsamos si todavía estaba reservado.
    if matches!(status, PayoutStatus::Pending | PayoutStatus::Processing) {
        adjust_escrow_in_tx(&mut tx, company_id, amount).await?;
    }
    sqlx::query(
        "UPDATE payouts SET status = 'failed', error_message = $2 WHERE id = $1",
    )
    .bind(id)
    .bind(error_message)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

/// Re-intenta un payout en `failed`: cambia el método de pago si aplica,
/// debita escrow ahora y vuelve a `pending`. Falla si escrow insuficiente.
pub async fn retry_failed(
    pool: &PgPool,
    id: PayoutId,
    new_payment_method_id: PaymentMethodId,
    new_rail: PaymentRail,
) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    let row: Option<(CompanyId, i64, PayoutStatus)> = sqlx::query_as(
        "SELECT company_id, amount_cents, status FROM payouts WHERE id = $1 FOR UPDATE",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((company_id, amount, status)) = row else {
        return Err(sqlx::Error::RowNotFound);
    };
    if !matches!(status, PayoutStatus::Failed) {
        // Sólo se reintenta lo que fracasó.
        return Err(sqlx::Error::Protocol("payout no está en failed".into()));
    }
    adjust_escrow_in_tx(&mut tx, company_id, -amount).await?;
    sqlx::query(
        "UPDATE payouts SET status = 'pending', payment_method_id = $2, rail = $3, \
                            error_message = NULL \
         WHERE id = $1",
    )
    .bind(id)
    .bind(new_payment_method_id)
    .bind(new_rail)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}
