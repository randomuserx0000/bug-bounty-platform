use async_trait::async_trait;
use lettre::{
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
    message::header::ContentType,
};
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
    #[error("smtp error: {0}")]
    Smtp(#[from] lettre::transport::smtp::Error),
    #[error("build error: {0}")]
    Build(#[from] lettre::error::Error),
    #[error("address error: {0}")]
    Address(#[from] lettre::address::AddressError),
}

#[async_trait]
pub trait EmailSender: Send + Sync {
    async fn send(&self, email: &Email) -> Result<(), EmailError>;
}

pub type SharedEmailSender = Arc<dyn EmailSender>;

/// Stub que no envía nada — sólo loggea. Pensado para dev y para tests.
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

pub struct SmtpSender {
    transport: AsyncSmtpTransport<Tokio1Executor>,
    from: String,
}

impl SmtpSender {
    /// Conecta sin TLS al servidor SMTP local (Postfix en 127.0.0.1:25).
    pub fn new(host: &str, port: u16, from: String) -> Self {
        let transport = AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(host)
            .port(port)
            .build();
        Self { transport, from }
    }
}

#[async_trait]
impl EmailSender for SmtpSender {
    async fn send(&self, email: &Email) -> Result<(), EmailError> {
        let msg = Message::builder()
            .from(self.from.parse()?)
            .to(email.to.parse()?)
            .subject(&email.subject)
            .header(ContentType::TEXT_HTML)
            .body(email.html_body.clone())?;
        self.transport.send(msg).await?;
        Ok(())
    }
}
