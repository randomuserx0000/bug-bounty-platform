//! Object storage para attachments de reports.
//!
//! Hoy: MinIO local en docker-compose, S3/Hetzner en prod. La crate
//! `aws-sdk-s3` habla con cualquier endpoint S3-compatible.
//!
//! Para MVP usamos **proxy upload**: el browser sube vía multipart a axum,
//! axum calcula sha256 + valida tamaño + sube a MinIO. Más simple que
//! presigned PUT (sin CORS, sha256 confiable). Costo: archivos pasan por
//! memoria de axum. Hay límite duro de 50 MB.
//!
//! Cuando aparezca un firmware de 500 MB, migramos a presigned PUT.

use aws_credential_types::Credentials;
use aws_sdk_s3::config::Region;
use aws_sdk_s3::primitives::ByteStream;
use aws_sdk_s3::Client;
use bytes::Bytes;
use secrecy::ExposeSecret;

use crate::config::Config;

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("not found")]
    NotFound,
    #[error("upstream: {0}")]
    Upstream(String),
}

#[derive(Clone)]
pub struct S3Storage {
    client: Client,
    bucket: String,
}

impl S3Storage {
    pub async fn from_config(cfg: &Config) -> anyhow::Result<Self> {
        let creds = Credentials::new(
            cfg.s3_access_key.expose_secret().to_string(),
            cfg.s3_secret_key.expose_secret().to_string(),
            None, // session token
            None, // expiry
            "bugbounty-config",
        );

        let sdk_config = aws_config::defaults(aws_config::BehaviorVersion::latest())
            .region(Region::new(cfg.s3_region.clone()))
            .endpoint_url(cfg.s3_endpoint.clone())
            .credentials_provider(creds)
            .load()
            .await;

        let s3_config = aws_sdk_s3::config::Builder::from(&sdk_config)
            .force_path_style(cfg.s3_force_path_style)
            .build();

        let client = Client::from_conf(s3_config);

        // Verificar que el bucket existe — falla temprano en arranque si
        // MinIO no está disponible o el bucket no se creó.
        client
            .head_bucket()
            .bucket(&cfg.s3_bucket)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("bucket '{}' no accesible: {e}", cfg.s3_bucket))?;

        Ok(Self { client, bucket: cfg.s3_bucket.clone() })
    }

    pub async fn put(
        &self,
        key: &str,
        body: Bytes,
        content_type: &str,
    ) -> Result<(), StorageError> {
        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(key)
            .body(ByteStream::from(body))
            .content_type(content_type)
            .send()
            .await
            .map_err(|e| StorageError::Upstream(format!("put {key}: {e}")))?;
        Ok(())
    }

    /// Descarga el objeto completo a memoria. Aceptable para MVP con
    /// límite de 50 MB; cuando aparezcan archivos grandes, refactorizar a
    /// `axum::body::Body::from_stream` sobre `ByteStream`.
    pub async fn get_bytes(
        &self,
        key: &str,
    ) -> Result<(Bytes, Option<String>), StorageError> {
        let out = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| {
                if e.to_string().contains("NoSuchKey") {
                    StorageError::NotFound
                } else {
                    StorageError::Upstream(format!("get {key}: {e}"))
                }
            })?;

        let ct = out.content_type().map(str::to_string);
        let agg = out
            .body
            .collect()
            .await
            .map_err(|e| StorageError::Upstream(format!("read {key}: {e}")))?;
        Ok((agg.into_bytes(), ct))
    }

    pub async fn delete(&self, key: &str) -> Result<(), StorageError> {
        self.client
            .delete_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| StorageError::Upstream(format!("delete {key}: {e}")))?;
        Ok(())
    }
}
