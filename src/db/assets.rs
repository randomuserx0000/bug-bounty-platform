//! Queries de `assets`.

use sqlx::PgPool;
use sqlx::types::JsonValue;

use crate::domain::asset::{AssetRecord, AssetSeverityCap, AssetType};
use crate::domain::ids::{AssetId, ProgramId};

const COLUMNS: &str = "id, program_id, asset_type, label, target, in_scope, \
                       severity_cap, notes_md, created_at";

pub async fn list_for_program(
    pool: &PgPool,
    program_id: ProgramId,
) -> Result<Vec<AssetRecord>, sqlx::Error> {
    let sql = format!(
        "SELECT {COLUMNS} FROM assets WHERE program_id = $1 \
         ORDER BY in_scope DESC, asset_type, created_at DESC"
    );
    sqlx::query_as::<_, AssetRecord>(&sql)
        .bind(program_id)
        .fetch_all(pool)
        .await
}

pub struct NewAsset<'a> {
    pub program_id: ProgramId,
    pub asset_type: AssetType,
    pub label: &'a str,
    pub target: &'a JsonValue,
    pub in_scope: bool,
    pub severity_cap: AssetSeverityCap,
    pub notes_md: Option<&'a str>,
}

pub async fn create(pool: &PgPool, a: NewAsset<'_>) -> Result<AssetId, sqlx::Error> {
    let id = AssetId::new();
    sqlx::query(
        "INSERT INTO assets
            (id, program_id, asset_type, label, target, in_scope, severity_cap, notes_md)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(id)
    .bind(a.program_id)
    .bind(a.asset_type)
    .bind(a.label)
    .bind(a.target)
    .bind(a.in_scope)
    .bind(a.severity_cap)
    .bind(a.notes_md)
    .execute(pool)
    .await?;
    Ok(id)
}

pub async fn delete(
    pool: &PgPool,
    id: AssetId,
    program_id: ProgramId,
) -> Result<u64, sqlx::Error> {
    let res = sqlx::query("DELETE FROM assets WHERE id = $1 AND program_id = $2")
        .bind(id)
        .bind(program_id)
        .execute(pool)
        .await?;
    Ok(res.rows_affected())
}
