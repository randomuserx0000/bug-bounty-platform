//! Queries de `reports`.

use sqlx::PgPool;

use crate::domain::ids::{AssetId, ProgramId, ReportId, UserId};
use crate::domain::report::{ReportRecord, ReportSeverity, ReportState};

const COLUMNS: &str = "id, public_id, program_id, asset_id, reporter_id, title, \
                       description_md, impact_md, repro_md, cwe, cvss_vector, \
                       severity, state, assigned_to, bounty_amount_cents, created_at";

pub async fn find_by_id(
    pool: &PgPool,
    id: ReportId,
) -> Result<Option<ReportRecord>, sqlx::Error> {
    let sql = format!("SELECT {COLUMNS} FROM reports WHERE id = $1");
    sqlx::query_as::<_, ReportRecord>(&sql)
        .bind(id)
        .fetch_optional(pool)
        .await
}

pub async fn find_by_public_id(
    pool: &PgPool,
    public_id: &str,
) -> Result<Option<ReportRecord>, sqlx::Error> {
    let sql = format!("SELECT {COLUMNS} FROM reports WHERE public_id = $1");
    sqlx::query_as::<_, ReportRecord>(&sql)
        .bind(public_id)
        .fetch_optional(pool)
        .await
}

pub async fn list_for_reporter(
    pool: &PgPool,
    reporter_id: UserId,
) -> Result<Vec<ReportRecord>, sqlx::Error> {
    let sql = format!(
        "SELECT {COLUMNS} FROM reports WHERE reporter_id = $1 ORDER BY created_at DESC"
    );
    sqlx::query_as::<_, ReportRecord>(&sql)
        .bind(reporter_id)
        .fetch_all(pool)
        .await
}

pub async fn list_for_program(
    pool: &PgPool,
    program_id: ProgramId,
) -> Result<Vec<ReportRecord>, sqlx::Error> {
    let sql = format!(
        "SELECT {COLUMNS} FROM reports WHERE program_id = $1 ORDER BY created_at DESC"
    );
    sqlx::query_as::<_, ReportRecord>(&sql)
        .bind(program_id)
        .fetch_all(pool)
        .await
}

pub struct NewReport<'a> {
    pub program_id: ProgramId,
    pub asset_id: Option<AssetId>,
    pub reporter_id: UserId,
    pub title: &'a str,
    pub description_md: &'a str,
    pub impact_md: Option<&'a str>,
    pub repro_md: Option<&'a str>,
    pub cwe: Option<&'a str>,
    pub cvss_vector: Option<&'a str>,
    pub severity: ReportSeverity,
}

/// Crea un report nuevo en estado `new`. Genera `public_id` desde la
/// secuencia global y prefijea con el año actual.
pub async fn create(
    pool: &PgPool,
    n: NewReport<'_>,
) -> Result<(ReportId, String), sqlx::Error> {
    let id = ReportId::new();
    let seq: i64 = sqlx::query_scalar("SELECT nextval('reports_public_seq')")
        .fetch_one(pool)
        .await?;
    let year = time::OffsetDateTime::now_utc().year();
    let public_id = format!("VE-{year}-{seq:05}");

    sqlx::query(
        "INSERT INTO reports
            (id, public_id, program_id, asset_id, reporter_id,
             title, description_md, impact_md, repro_md,
             cwe, cvss_vector, severity)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)",
    )
    .bind(id)
    .bind(&public_id)
    .bind(n.program_id)
    .bind(n.asset_id)
    .bind(n.reporter_id)
    .bind(n.title)
    .bind(n.description_md)
    .bind(n.impact_md)
    .bind(n.repro_md)
    .bind(n.cwe)
    .bind(n.cvss_vector)
    .bind(n.severity)
    .execute(pool)
    .await?;

    Ok((id, public_id))
}

/// Actualiza el estado y los timestamps derivados (`first_response_at`,
/// `triaged_at`, `resolved_at`) en función de la transición.
pub async fn update_state(
    pool: &PgPool,
    id: ReportId,
    new_state: ReportState,
) -> Result<(), sqlx::Error> {
    // first_response_at: se setea la primera vez que sale de `new`.
    // triaged_at: cuando entra a accepted/duplicate/not_applicable/informative.
    // resolved_at: cuando entra a resolved.
    let sets_triaged = matches!(
        new_state,
        ReportState::Accepted
            | ReportState::Duplicate
            | ReportState::NotApplicable
            | ReportState::Informative
    );
    sqlx::query(
        "UPDATE reports SET
            state = $2,
            updated_at = now(),
            first_response_at = COALESCE(first_response_at,
                CASE WHEN $2 <> 'new' THEN now() ELSE NULL END),
            triaged_at = COALESCE(triaged_at,
                CASE WHEN $3 THEN now() ELSE NULL END),
            resolved_at = COALESCE(resolved_at,
                CASE WHEN $2 = 'resolved' THEN now() ELSE NULL END),
            disclosed_at = COALESCE(disclosed_at,
                CASE WHEN $2 = 'disclosed' THEN now() ELSE NULL END)
         WHERE id = $1",
    )
    .bind(id)
    .bind(new_state)
    .bind(sets_triaged)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn update_severity(
    pool: &PgPool,
    id: ReportId,
    severity: ReportSeverity,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE reports SET severity = $2, updated_at = now() WHERE id = $1")
        .bind(id)
        .bind(severity)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn update_bounty(
    pool: &PgPool,
    id: ReportId,
    cents: i32,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE reports SET bounty_amount_cents = $2, updated_at = now() WHERE id = $1",
    )
    .bind(id)
    .bind(cents)
    .execute(pool)
    .await?;
    Ok(())
}
