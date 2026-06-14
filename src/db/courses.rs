//! Queries de `course_requests` (solicitudes del curso Analista de
//! Ciberseguridad). v1 de la academia: solo captación de leads; el LMS
//! completo está diseñado en docs/osint-academy.md.

use sqlx::PgPool;

use crate::domain::ids::{CourseRequestId, UserId};

pub struct NewCourseRequest<'a> {
    /// Cuenta enlazada si el visitante tenía sesión al solicitar.
    pub user_id: Option<UserId>,
    pub name: &'a str,
    pub email: &'a str,
    pub experience: &'a str,
    pub message: &'a str,
    pub course_slug: &'a str,
}

pub async fn create_request(
    pool: &PgPool,
    n: NewCourseRequest<'_>,
) -> Result<CourseRequestId, sqlx::Error> {
    let id = CourseRequestId::new();
    sqlx::query(
        "INSERT INTO course_requests (id, user_id, name, email, experience, message, course_slug)
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(id)
    .bind(n.user_id)
    .bind(n.name)
    .bind(n.email)
    .bind(n.experience)
    .bind(n.message)
    .bind(n.course_slug)
    .execute(pool)
    .await?;
    Ok(id)
}
