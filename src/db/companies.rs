//! Queries de `companies` + `company_members`.

use sqlx::PgPool;

use crate::domain::company::{CompanyMembership, CompanyRecord, CompanyRole, CompanyStatus};
use crate::domain::ids::{CompanyId, UserId};

const COLUMNS: &str = "id, slug, legal_name, display_name, country_code, website, \
                       description, status, escrow_balance_cents, created_at";

pub async fn find_by_id(
    pool: &PgPool,
    id: CompanyId,
) -> Result<Option<CompanyRecord>, sqlx::Error> {
    let sql = format!("SELECT {COLUMNS} FROM companies WHERE id = $1");
    sqlx::query_as::<_, CompanyRecord>(&sql)
        .bind(id)
        .fetch_optional(pool)
        .await
}

pub async fn find_by_slug(
    pool: &PgPool,
    slug: &str,
) -> Result<Option<CompanyRecord>, sqlx::Error> {
    let sql = format!("SELECT {COLUMNS} FROM companies WHERE slug = $1");
    sqlx::query_as::<_, CompanyRecord>(&sql)
        .bind(slug)
        .fetch_optional(pool)
        .await
}

/// Lista las companies de las que `user_id` es miembro, ordenadas por
/// nombre. Devuelve también el rol del usuario en cada una.
#[derive(Debug, sqlx::FromRow)]
struct CompanyWithRoleRow {
    pub id: CompanyId,
    pub slug: String,
    pub legal_name: String,
    pub display_name: String,
    pub country_code: Option<String>,
    pub website: Option<String>,
    pub description: Option<String>,
    pub status: CompanyStatus,
    pub escrow_balance_cents: i64,
    pub created_at: sqlx::types::time::OffsetDateTime,
    pub member_role: String,
}

pub async fn list_for_user(
    pool: &PgPool,
    user_id: UserId,
) -> Result<Vec<(CompanyRecord, CompanyRole)>, sqlx::Error> {
    let rows: Vec<CompanyWithRoleRow> = sqlx::query_as(
        "SELECT c.id, c.slug, c.legal_name, c.display_name, c.country_code, c.website,
                c.description, c.status, c.escrow_balance_cents, c.created_at,
                cm.role AS member_role
         FROM companies c
         JOIN company_members cm ON cm.company_id = c.id
         WHERE cm.user_id = $1
         ORDER BY c.display_name",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .filter_map(|r| {
            CompanyRole::from_str(&r.member_role).map(|role| {
                (
                    CompanyRecord {
                        id: r.id,
                        slug: r.slug,
                        legal_name: r.legal_name,
                        display_name: r.display_name,
                        country_code: r.country_code,
                        website: r.website,
                        description: r.description,
                        status: r.status,
                        escrow_balance_cents: r.escrow_balance_cents,
                        created_at: r.created_at,
                    },
                    role,
                )
            })
        })
        .collect())
}

pub struct NewCompany<'a> {
    pub slug: &'a str,
    pub legal_name: &'a str,
    pub display_name: &'a str,
    pub country_code: Option<&'a str>,
    pub website: Option<&'a str>,
    pub description: Option<&'a str>,
}

/// Crea la company **y** la membresía del creador como `owner` en una sola
/// transacción. Sin esto, una falla a mitad de camino dejaría una company
/// huérfana que nadie puede administrar.
pub async fn create_with_owner(
    pool: &PgPool,
    user_id: UserId,
    n: NewCompany<'_>,
) -> Result<CompanyId, sqlx::Error> {
    let id = CompanyId::new();
    let mut tx = pool.begin().await?;
    sqlx::query(
        "INSERT INTO companies
             (id, slug, legal_name, display_name, country_code, website, description, status)
         VALUES ($1, $2, $3, $4, $5, $6, $7, 'active')",
    )
    .bind(id)
    .bind(n.slug)
    .bind(n.legal_name)
    .bind(n.display_name)
    .bind(n.country_code)
    .bind(n.website)
    .bind(n.description)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO company_members (company_id, user_id, role)
         VALUES ($1, $2, 'owner')",
    )
    .bind(id)
    .bind(user_id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(id)
}

pub async fn membership(
    pool: &PgPool,
    company_id: CompanyId,
    user_id: UserId,
) -> Result<Option<CompanyMembership>, sqlx::Error> {
    let row: Option<(String,)> = sqlx::query_as(
        "SELECT role FROM company_members WHERE company_id = $1 AND user_id = $2",
    )
    .bind(company_id)
    .bind(user_id)
    .fetch_optional(pool)
    .await?;
    Ok(row
        .and_then(|(r,)| CompanyRole::from_str(&r))
        .map(|role| CompanyMembership { company_id, user_id, role }))
}

/// Lee el saldo actual de escrow (USD cents).
pub async fn escrow_balance(
    pool: &PgPool,
    company_id: CompanyId,
) -> Result<i64, sqlx::Error> {
    let row: (i64,) = sqlx::query_as(
        "SELECT escrow_balance_cents FROM companies WHERE id = $1",
    )
    .bind(company_id)
    .fetch_one(pool)
    .await?;
    Ok(row.0)
}

/// Ajusta el escrow sumando `delta_cents` (negativo = débito, positivo =
/// crédito). El CHECK del schema (`>= 0`) impide que quede negativo: si
/// no hay saldo suficiente, Postgres devuelve error y la transacción
/// caller hace rollback.
///
/// IMPORTANTE: esta función NO crea su propia transacción — espera ser
/// llamada dentro de una. Así escrow + payout se mueven juntos.
pub async fn adjust_escrow_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    company_id: CompanyId,
    delta_cents: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE companies SET escrow_balance_cents = escrow_balance_cents + $2,
                              updated_at = now()
         WHERE id = $1",
    )
    .bind(company_id)
    .bind(delta_cents)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Versión one-shot fuera de transacción (para depósitos manuales de admin).
pub async fn adjust_escrow(
    pool: &PgPool,
    company_id: CompanyId,
    delta_cents: i64,
) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    adjust_escrow_in_tx(&mut tx, company_id, delta_cents).await?;
    tx.commit().await?;
    Ok(())
}

/// Emails de los owners + admins de una company. Útil para notificaciones.
pub async fn owner_emails(
    pool: &PgPool,
    company_id: CompanyId,
) -> Result<Vec<String>, sqlx::Error> {
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT u.email
         FROM company_members cm
         JOIN users u ON u.id = cm.user_id
         WHERE cm.company_id = $1 AND cm.role IN ('owner', 'admin')",
    )
    .bind(company_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|(e,)| e).collect())
}

#[allow(dead_code)]
pub async fn count_active(pool: &PgPool) -> Result<i64, sqlx::Error> {
    let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM companies WHERE status = $1")
        .bind(CompanyStatus::Active)
        .fetch_one(pool)
        .await?;
    Ok(row.0)
}
