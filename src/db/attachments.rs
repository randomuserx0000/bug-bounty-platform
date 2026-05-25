//! Queries de `report_attachments`.

use sqlx::PgPool;
use time::OffsetDateTime;

use crate::domain::ids::{AttachmentId, ReportId, UserId};

#[derive(Debug, sqlx::FromRow)]
pub struct AttachmentRow {
    pub id: AttachmentId,
    pub report_id: ReportId,
    pub uploader_id: UserId,
    pub filename: String,
    pub mime: String,
    pub size_bytes: i64,
    pub sha256: Vec<u8>,
    pub storage_key: String,
    pub kind: String,
    pub created_at: OffsetDateTime,
}

pub async fn list_for_report(
    pool: &PgPool,
    report_id: ReportId,
) -> Result<Vec<AttachmentRow>, sqlx::Error> {
    sqlx::query_as::<_, AttachmentRow>(
        "SELECT id, report_id, uploader_id, filename, mime, size_bytes, sha256, storage_key, kind, created_at
         FROM report_attachments WHERE report_id = $1 ORDER BY created_at ASC",
    )
    .bind(report_id)
    .fetch_all(pool)
    .await
}

pub async fn find_by_id(
    pool: &PgPool,
    id: AttachmentId,
) -> Result<Option<AttachmentRow>, sqlx::Error> {
    sqlx::query_as::<_, AttachmentRow>(
        "SELECT id, report_id, uploader_id, filename, mime, size_bytes, sha256, storage_key, kind, created_at
         FROM report_attachments WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
}

pub struct NewAttachment<'a> {
    pub report_id: ReportId,
    pub uploader_id: UserId,
    pub filename: &'a str,
    pub mime: &'a str,
    pub size_bytes: i64,
    pub sha256: &'a [u8],
    pub storage_key: &'a str,
    pub kind: &'a str,
}

pub async fn create(pool: &PgPool, a: NewAttachment<'_>) -> Result<AttachmentId, sqlx::Error> {
    let id = AttachmentId::new();
    sqlx::query(
        "INSERT INTO report_attachments
            (id, report_id, uploader_id, filename, mime, size_bytes, sha256, storage_key, kind)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
    )
    .bind(id)
    .bind(a.report_id)
    .bind(a.uploader_id)
    .bind(a.filename)
    .bind(a.mime)
    .bind(a.size_bytes)
    .bind(a.sha256)
    .bind(a.storage_key)
    .bind(a.kind)
    .execute(pool)
    .await?;
    Ok(id)
}

pub async fn delete(
    pool: &PgPool,
    id: AttachmentId,
    report_id: ReportId,
) -> Result<u64, sqlx::Error> {
    let res = sqlx::query("DELETE FROM report_attachments WHERE id = $1 AND report_id = $2")
        .bind(id)
        .bind(report_id)
        .execute(pool)
        .await?;
    Ok(res.rows_affected())
}
