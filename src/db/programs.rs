//! Queries de `programs`.

use sqlx::PgPool;

use crate::domain::ids::{CompanyId, ProgramId};
use crate::domain::program::{ProgramRecord, ProgramStatus, ProgramVisibility};

const COLUMNS: &str = "id, company_id, slug, name, summary, policy_md, visibility, status, \
                       bounty_low_cents, bounty_medium_cents, bounty_high_cents, \
                       bounty_critical_cents, allows_redteam, allows_hardware, \
                       created_at, launched_at";

pub async fn find_by_id(
    pool: &PgPool,
    id: ProgramId,
) -> Result<Option<ProgramRecord>, sqlx::Error> {
    let sql = format!("SELECT {COLUMNS} FROM programs WHERE id = $1");
    sqlx::query_as::<_, ProgramRecord>(&sql)
        .bind(id)
        .fetch_optional(pool)
        .await
}

pub async fn find_by_company_and_slug(
    pool: &PgPool,
    company_id: CompanyId,
    slug: &str,
) -> Result<Option<ProgramRecord>, sqlx::Error> {
    let sql = format!("SELECT {COLUMNS} FROM programs WHERE company_id = $1 AND slug = $2");
    sqlx::query_as::<_, ProgramRecord>(&sql)
        .bind(company_id)
        .bind(slug)
        .fetch_optional(pool)
        .await
}

pub async fn list_for_company(
    pool: &PgPool,
    company_id: CompanyId,
) -> Result<Vec<ProgramRecord>, sqlx::Error> {
    let sql = format!(
        "SELECT {COLUMNS} FROM programs WHERE company_id = $1 ORDER BY created_at DESC"
    );
    sqlx::query_as::<_, ProgramRecord>(&sql)
        .bind(company_id)
        .fetch_all(pool)
        .await
}

/// Programas en listado público: visibility=public + status=public.
/// Devuelve también el display_name de la company para mostrar en la lista.
pub async fn list_public(pool: &PgPool) -> Result<Vec<PublicProgramRow>, sqlx::Error> {
    sqlx::query_as::<_, PublicProgramRow>(
        "SELECT p.id, p.slug, p.name, p.summary,
                p.bounty_low_cents, p.bounty_critical_cents,
                c.slug AS company_slug, c.display_name AS company_name
         FROM programs p
         JOIN companies c ON c.id = p.company_id
         WHERE p.visibility = 'public' AND p.status = 'public'
         ORDER BY p.launched_at DESC NULLS LAST, p.created_at DESC",
    )
    .fetch_all(pool)
    .await
}

#[derive(Debug, sqlx::FromRow)]
pub struct PublicProgramRow {
    pub id: ProgramId,
    pub slug: String,
    pub name: String,
    pub summary: Option<String>,
    pub bounty_low_cents: Option<i32>,
    pub bounty_critical_cents: Option<i32>,
    pub company_slug: String,
    pub company_name: String,
}

pub struct NewProgram<'a> {
    pub company_id: CompanyId,
    pub slug: &'a str,
    pub name: &'a str,
    pub summary: Option<&'a str>,
    pub policy_md: &'a str,
    pub visibility: ProgramVisibility,
    pub status: ProgramStatus,
    pub bounty_low_cents: Option<i32>,
    pub bounty_medium_cents: Option<i32>,
    pub bounty_high_cents: Option<i32>,
    pub bounty_critical_cents: Option<i32>,
    pub allows_redteam: bool,
    pub allows_hardware: bool,
}

pub async fn create(pool: &PgPool, p: NewProgram<'_>) -> Result<ProgramId, sqlx::Error> {
    let id = ProgramId::new();
    sqlx::query(
        "INSERT INTO programs
            (id, company_id, slug, name, summary, policy_md, visibility, status,
             bounty_low_cents, bounty_medium_cents, bounty_high_cents, bounty_critical_cents,
             allows_redteam, allows_hardware,
             launched_at)
         VALUES
            ($1, $2, $3, $4, $5, $6, $7, $8,
             $9, $10, $11, $12,
             $13, $14,
             CASE WHEN $8 = 'public' THEN now() ELSE NULL END)",
    )
    .bind(id)
    .bind(p.company_id)
    .bind(p.slug)
    .bind(p.name)
    .bind(p.summary)
    .bind(p.policy_md)
    .bind(p.visibility)
    .bind(p.status)
    .bind(p.bounty_low_cents)
    .bind(p.bounty_medium_cents)
    .bind(p.bounty_high_cents)
    .bind(p.bounty_critical_cents)
    .bind(p.allows_redteam)
    .bind(p.allows_hardware)
    .execute(pool)
    .await?;
    Ok(id)
}
