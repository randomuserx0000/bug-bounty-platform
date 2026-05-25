//! Tipo de error único de la app + conversión a respuesta HTTP.
//!
//! Regla: ningún handler devuelve `anyhow::Error` directamente. Todos
//! devuelven `Result<T, AppError>` para que el mapeo a status code sea
//! explícito y nunca filtremos detalles internos al cliente.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("not found")]
    NotFound,

    #[error("unauthorized")]
    Unauthorized,

    #[error("forbidden")]
    Forbidden,

    #[error("conflict: {0}")]
    Conflict(String),

    #[error("validation: {0}")]
    Validation(String),

    /// Errores externos que no deben filtrarse: DB caída, IO, etc.
    /// Se loggean con detalle pero al cliente solo le llega 500.
    #[error(transparent)]
    Internal(#[from] anyhow::Error),

    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, msg) = match &self {
            AppError::NotFound       => (StatusCode::NOT_FOUND, "not found".to_string()),
            AppError::Unauthorized   => (StatusCode::UNAUTHORIZED, "unauthorized".to_string()),
            AppError::Forbidden      => (StatusCode::FORBIDDEN, "forbidden".to_string()),
            AppError::Conflict(m)    => (StatusCode::CONFLICT, m.clone()),
            AppError::Validation(m)  => (StatusCode::UNPROCESSABLE_ENTITY, m.clone()),
            AppError::Db(e) => {
                tracing::error!(error = ?e, "db error");
                (StatusCode::INTERNAL_SERVER_ERROR, "internal error".to_string())
            }
            AppError::Internal(e) => {
                tracing::error!(error = ?e, "internal error");
                (StatusCode::INTERNAL_SERVER_ERROR, "internal error".to_string())
            }
        };
        (status, msg).into_response()
    }
}

pub type AppResult<T> = Result<T, AppError>;
