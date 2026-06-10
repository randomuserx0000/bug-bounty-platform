//! Configuración cargada del entorno.
//!
//! Toda variable que cambie entre dev/staging/prod vive aquí. No hay
//! defaults peligrosos: si falta algo crítico, fallamos al arrancar.

use axum_extra::extract::cookie::Key;
use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    /// "0.0.0.0:8080" en prod, "127.0.0.1:8080" en dev.
    #[serde(default = "default_bind")]
    pub bind_addr: String,

    /// URL Postgres. p.ej. postgres://user:pass@host:5432/bugbounty
    pub database_url: SecretString,

    /// Pool: número máximo de conexiones a Postgres.
    #[serde(default = "default_db_max_conn")]
    pub db_max_conn: u32,

    /// Key simétrica (hex) para cifrar `payment_methods.details_enc`.
    /// 32 bytes en hex = 64 chars. Rota con cuidado (requiere re-cifrar).
    pub payment_methods_key_hex: SecretString,

    /// Key para cookies firmadas. 64 bytes en hex (128 chars).
    /// genera con: openssl rand -hex 64
    pub cookie_key_hex: SecretString,

    /// Origen público (para enlaces en emails, etc.). Sin trailing slash.
    /// Si empieza con "https", las cookies se marcan como Secure.
    #[serde(default = "default_public_url")]
    pub public_url: String,

    // ---- Object storage (MinIO en dev, S3/Hetzner en prod) ----
    #[serde(default = "default_s3_endpoint")]
    pub s3_endpoint: String,
    #[serde(default = "default_s3_region")]
    pub s3_region: String,
    #[serde(default = "default_s3_bucket")]
    pub s3_bucket: String,
    pub s3_access_key: SecretString,
    pub s3_secret_key: SecretString,
    /// `true` para MinIO/Hetzner (path-style). `false` para AWS S3 nativo.
    #[serde(default = "default_true")]
    pub s3_force_path_style: bool,

    // ---- OAuth (Google) ----
    /// Si están vacíos, el botón "Continuar con Google" no se muestra.
    #[serde(default)]
    pub google_client_id: String,
    #[serde(default)]
    pub google_client_secret: SecretString,

    /// Correo que recibe el aviso cuando entra un nuevo informe OSINT a revisar
    /// (env `OSINT_NOTIFY_EMAIL`). Vacío = no se envía aviso (el informe igual
    /// queda en BD + audit log).
    #[serde(default)]
    pub osint_notify_email: String,
}


fn default_bind() -> String { "127.0.0.1:8080".into() }
fn default_db_max_conn() -> u32 { 10 }
fn default_public_url() -> String { "http://localhost:8080".into() }
fn default_s3_endpoint() -> String { "http://localhost:9000".into() }
fn default_s3_region() -> String { "us-east-1".into() }
fn default_s3_bucket() -> String { "bb-attachments".into() }
fn default_true() -> bool { true }

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let cfg: Self = envy::from_env()
            .map_err(|e| anyhow::anyhow!("config inválida: {e}"))?;
        Ok(cfg)
    }

    /// Decodifica `cookie_key_hex` a una `Key` lista para usar.
    /// Falla si el hex es inválido o no tiene exactamente 64 bytes.
    pub fn cookie_key(&self) -> anyhow::Result<Key> {
        let bytes = hex::decode(self.cookie_key_hex.expose_secret().trim())
            .map_err(|e| anyhow::anyhow!("COOKIE_KEY_HEX inválido: {e}"))?;
        if bytes.len() < 64 {
            anyhow::bail!("COOKIE_KEY_HEX debe tener al menos 64 bytes ({} actual)", bytes.len());
        }
        Ok(Key::from(&bytes))
    }

    pub fn cookie_secure(&self) -> bool {
        self.public_url.starts_with("https://")
    }

    /// Dirección de aviso de OSINT, si está configurada y no vacía.
    pub fn osint_notify_email(&self) -> Option<&str> {
        let e = self.osint_notify_email.trim();
        if e.is_empty() { None } else { Some(e) }
    }

    pub fn google_oauth_enabled(&self) -> bool {
        !self.google_client_id.trim().is_empty()
            && !self.google_client_secret.expose_secret().trim().is_empty()
    }

    /// URL pública del callback. Debe coincidir EXACTAMENTE con lo que se
    /// registró en Google Cloud Console.
    pub fn google_redirect_uri(&self) -> String {
        format!("{}/auth/google/callback", self.public_url.trim_end_matches('/'))
    }
}
