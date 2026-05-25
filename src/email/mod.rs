//! Envío de email transaccional.
//!
//! El proveedor real (Postmark / Resend / SES) requiere dominio verificado
//! y credenciales. Hasta entonces vivimos con [`LogOnly`], que escribe el
//! email al log y devuelve Ok — suficiente para desarrollar el flujo de
//! reports y revisar que los hooks disparen en los momentos correctos.
//!
//! El día que elijamos proveedor: añadir una impl que llame su API HTTP
//! (con `reqwest`) y cambiar el constructor en `main.rs`. Los handlers
//! no se enteran.

use async_trait::async_trait;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct Email {
    pub to: String,
    pub subject: String,
    pub html_body: String,
    pub text_body: String,
}

#[derive(Debug, thiserror::Error)]
pub enum EmailError {
    #[error("provider error: {0}")]
    Provider(String),
}

#[async_trait]
pub trait EmailSender: Send + Sync {
    async fn send(&self, email: &Email) -> Result<(), EmailError>;
}

pub type SharedEmailSender = Arc<dyn EmailSender>;

/// Stub que no envía nada — sólo loggea. Pensado para dev y para tests.
/// En prod cae el deploy y forzamos a inyectar un sender real.
pub struct LogOnly;

#[async_trait]
impl EmailSender for LogOnly {
    async fn send(&self, e: &Email) -> Result<(), EmailError> {
        tracing::info!(
            target = "email.log_only",
            to = %e.to,
            subject = %e.subject,
            "would send email"
        );
        tracing::debug!(text = %e.text_body, "email body");
        Ok(())
    }
}
