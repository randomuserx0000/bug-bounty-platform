//! Queries de `osint_reports`.
//!
//! La pieza no-trivial es `purchase`: una empresa compra un informe OSINT
//! aceptado. En una sola transacción debitamos su escrow por
//! `resale_price_cents` y marcamos el informe como `sold`. Si el escrow no
//! alcanza, el CHECK `escrow_balance_cents >= 0` rompe la transacción y el
//! caller muestra un error.
//!
//! NOTA (v1): el pago al investigador (`price_cents`, base $50) se registra en
//! el propio `osint_reports` y se muestra en su panel; el desembolso real al
//! researcher se gestiona operativamente. La integración con la tabla
//! `payouts` (ya preparada con `osint_report_id`) queda para una 2ª iteración.

use sqlx::PgPool;

use crate::db::companies::adjust_escrow_in_tx;
use crate::domain::ids::{CompanyId, OsintReportId, UserId};
use crate::domain::osint::{OsintCategory, OsintReportRecord};
use crate::domain::report::ReportSeverity;

const COLUMNS: &str = "id, public_id, researcher_id, subject_company_id, subject_name, \
                       title, category, criticality, summary, body_md, price_cents, \
                       resale_price_cents, status, reviewed_by, sold_to_company_id, \
                       sold_at, created_at";

pub async fn find_by_public_id(
    pool: &PgPool,
    public_id: &str,
) -> Result<Option<OsintReportRecord>, sqlx::Error> {
    let sql = format!("SELECT {COLUMNS} FROM osint_reports WHERE public_id = $1");
    sqlx::query_as::<_, OsintReportRecord>(&sql)
        .bind(public_id)
        .fetch_optional(pool)
        .await
}

pub async fn list_for_researcher(
    pool: &PgPool,
    researcher_id: UserId,
) -> Result<Vec<OsintReportRecord>, sqlx::Error> {
    let sql = format!(
        "SELECT {COLUMNS} FROM osint_reports WHERE researcher_id = $1 ORDER BY created_at DESC"
    );
    sqlx::query_as::<_, OsintReportRecord>(&sql)
        .bind(researcher_id)
        .fetch_all(pool)
        .await
}

/// Cola de revisión del admin: lo que está enviado o en revisión.
pub async fn list_for_review(pool: &PgPool) -> Result<Vec<OsintReportRecord>, sqlx::Error> {
    let sql = format!(
        "SELECT {COLUMNS} FROM osint_reports \
         WHERE status IN ('submitted','in_review') ORDER BY created_at ASC"
    );
    sqlx::query_as::<_, OsintReportRecord>(&sql)
        .fetch_all(pool)
        .await
}

/// Catálogo para una empresa: informes aceptados (a la venta) sobre ESA
/// empresa, más los que ya compró.
pub async fn list_catalog_for_company(
    pool: &PgPool,
    company_id: CompanyId,
) -> Result<Vec<OsintReportRecord>, sqlx::Error> {
    let sql = format!(
        "SELECT {COLUMNS} FROM osint_reports \
         WHERE subject_company_id = $1 AND status IN ('accepted','sold') \
         ORDER BY created_at DESC"
    );
    sqlx::query_as::<_, OsintReportRecord>(&sql)
        .bind(company_id)
        .fetch_all(pool)
        .await
}

pub struct NewOsintReport<'a> {
    pub researcher_id: UserId,
    pub subject_company_id: Option<CompanyId>,
    pub subject_name: &'a str,
    pub title: &'a str,
    pub category: OsintCategory,
    pub criticality: ReportSeverity,
    pub summary: &'a str,
    pub body_md: &'a str,
    pub price_cents: i32,
}

/// Crea un informe OSINT en estado `submitted`. Genera `public_id` desde la
/// secuencia global con prefijo del año actual ("OSINT-2026-00001").
pub async fn create(
    pool: &PgPool,
    n: NewOsintReport<'_>,
) -> Result<(OsintReportId, String), sqlx::Error> {
    let id = OsintReportId::new();
    let seq: i64 = sqlx::query_scalar("SELECT nextval('osint_public_seq')")
        .fetch_one(pool)
        .await?;
    let year = time::OffsetDateTime::now_utc().year();
    let public_id = format!("OSINT-{year}-{seq:05}");

    sqlx::query(
        "INSERT INTO osint_reports
            (id, public_id, researcher_id, subject_company_id, subject_name,
             title, category, criticality, summary, body_md, price_cents)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
    )
    .bind(id)
    .bind(&public_id)
    .bind(n.researcher_id)
    .bind(n.subject_company_id)
    .bind(n.subject_name)
    .bind(n.title)
    .bind(n.category)
    .bind(n.criticality)
    .bind(n.summary)
    .bind(n.body_md)
    .bind(n.price_cents)
    .execute(pool)
    .await?;

    Ok((id, public_id))
}

/// Acepta un informe: fija el precio de reventa, marca `accepted` y registra
/// el revisor. Solo válido desde submitted/in_review.
pub async fn accept(
    pool: &PgPool,
    id: OsintReportId,
    reviewed_by: UserId,
    resale_price_cents: i32,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE osint_reports
            SET status = 'accepted', resale_price_cents = $2, reviewed_by = $3,
                updated_at = now()
         WHERE id = $1 AND status IN ('submitted','in_review')",
    )
    .bind(id)
    .bind(resale_price_cents)
    .bind(reviewed_by)
    .execute(pool)
    .await?;
    Ok(())
}

/// Rechaza un informe. Solo válido desde submitted/in_review.
pub async fn reject(
    pool: &PgPool,
    id: OsintReportId,
    reviewed_by: UserId,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE osint_reports
            SET status = 'rejected', reviewed_by = $2, updated_at = now()
         WHERE id = $1 AND status IN ('submitted','in_review')",
    )
    .bind(id)
    .bind(reviewed_by)
    .execute(pool)
    .await?;
    Ok(())
}

/// Compra: debita el escrow de la empresa por `resale_cents` y marca el
/// informe como `sold` en una sola transacción. Si el escrow no alcanza, el
/// CHECK rompe la transacción y devolvemos error (el caller lo traduce a un
/// mensaje claro). Devuelve `false` si el informe no estaba `accepted` (carrera).
pub async fn purchase(
    pool: &PgPool,
    id: OsintReportId,
    buyer_company_id: CompanyId,
    resale_cents: i64,
) -> Result<bool, sqlx::Error> {
    let mut tx = pool.begin().await?;
    adjust_escrow_in_tx(&mut tx, buyer_company_id, -resale_cents).await?;
    let res = sqlx::query(
        "UPDATE osint_reports
            SET status = 'sold', sold_to_company_id = $2, sold_at = now(),
                updated_at = now()
         WHERE id = $1 AND status = 'accepted'",
    )
    .bind(id)
    .bind(buyer_company_id)
    .execute(&mut *tx)
    .await?;
    if res.rows_affected() == 0 {
        // No estaba accepted: revertimos el débito haciendo rollback.
        tx.rollback().await?;
        return Ok(false);
    }
    tx.commit().await?;
    Ok(true)
}
