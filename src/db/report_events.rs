//! Timeline de un report: comentarios + cambios de estado/severity/etc.

use sqlx::PgPool;
use sqlx::types::JsonValue;

use crate::domain::ids::{ReportEventId, ReportId, UserId};
use crate::domain::report::{EventType, ReportEventRecord};

pub async fn list_for_report(
    pool: &PgPool,
    report_id: ReportId,
    include_internal: bool,
) -> Result<Vec<ReportEventRecord>, sqlx::Error> {
    let sql = if include_internal {
        "SELECT id, report_id, actor_id, event_type, body_md, metadata, is_internal, created_at
         FROM report_events WHERE report_id = $1 ORDER BY created_at ASC"
    } else {
        "SELECT id, report_id, actor_id, event_type, body_md, metadata, is_internal, created_at
         FROM report_events WHERE report_id = $1 AND is_internal = FALSE
         ORDER BY created_at ASC"
    };
    sqlx::query_as::<_, ReportEventRecord>(sql)
        .bind(report_id)
        .fetch_all(pool)
        .await
}

pub struct NewEvent<'a> {
    pub report_id: ReportId,
    pub actor_id: Option<UserId>,
    pub event_type: EventType,
    pub body_md: Option<&'a str>,
    pub metadata: Option<JsonValue>,
    pub is_internal: bool,
}

pub async fn create(pool: &PgPool, e: NewEvent<'_>) -> Result<(), sqlx::Error> {
    let id = ReportEventId::new();
    sqlx::query(
        "INSERT INTO report_events
            (id, report_id, actor_id, event_type, body_md, metadata, is_internal)
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(id)
    .bind(e.report_id)
    .bind(e.actor_id)
    .bind(e.event_type.as_str())
    .bind(e.body_md)
    .bind(e.metadata)
    .bind(e.is_internal)
    .execute(pool)
    .await?;
    Ok(())
}
