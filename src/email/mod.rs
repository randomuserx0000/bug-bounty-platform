use async_trait::async_trait;
use lettre::{
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
    message::MultiPart,
    transport::smtp::authentication::Credentials,
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

/// Cómo se cifra la conexión con el servidor SMTP.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SmtpTls {
    /// Sin cifrado. Solo para un servidor local de confianza.
    None,
    /// Conexión en claro que se eleva a TLS (Gmail, Brevo: puerto 587).
    StartTls,
    /// TLS desde el inicio (puerto 465).
    Tls,
}

impl SmtpTls {
    pub fn parse(s: &str) -> Result<Self, String> {
        match s.trim().to_ascii_lowercase().as_str() {
            "" | "none" => Ok(Self::None),
            "starttls" => Ok(Self::StartTls),
            "tls" | "ssl" => Ok(Self::Tls),
            other => Err(format!("SMTP_TLS inválido: {other:?} (usa none, starttls o tls)")),
        }
    }
}

impl SmtpSender {
    pub fn new(
        host: &str,
        port: u16,
        from: String,
        username: &str,
        password: &str,
        tls: SmtpTls,
    ) -> Result<Self, EmailError> {
        let builder = match tls {
            SmtpTls::None => AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(host),
            SmtpTls::StartTls => AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(host)?,
            SmtpTls::Tls => AsyncSmtpTransport::<Tokio1Executor>::relay(host)?,
        };
        let mut builder = builder.port(port);
        if !username.is_empty() {
            builder = builder.credentials(Credentials::new(username.to_string(), password.to_string()));
        }
        Ok(Self { transport: builder.build(), from })
    }
}

impl SmtpSender {
    async fn send_inner(&self, email: &Email) -> Result<(), EmailError> {
        // Texto plano + HTML: los clientes eligen, y baja la probabilidad de spam.
        let msg = Message::builder()
            .from(self.from.parse()?)
            .to(email.to.parse()?)
            .subject(&email.subject)
            .multipart(MultiPart::alternative_plain_html(
                email.text_body.clone(),
                email.html_body.clone(),
            ))?;
        self.transport.send(msg).await?;
        Ok(())
    }
}

#[async_trait]
impl EmailSender for SmtpSender {
    async fn send(&self, email: &Email) -> Result<(), EmailError> {
        // Muchos llamadores descartan el resultado (`let _ = ...send()`), así que
        // los fallos se registran aquí para que siempre queden en los logs.
        let result = self.send_inner(email).await;
        match &result {
            Ok(()) => tracing::info!(to = %email.to, subject = %email.subject, "email sent"),
            Err(e) => tracing::error!(to = %email.to, subject = %email.subject, error = %e, "email send failed"),
        }
        result
    }
}
