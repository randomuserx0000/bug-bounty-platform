//! Tipos de la entidad `osint_reports`.
//!
//! Un informe OSINT lo envía un investigador, lo revisa un admin de la
//! plataforma, y si se acepta entra al catálogo para que la empresa-objetivo
//! lo compre. Flujo de estados corto:
//!
//! ```text
//! submitted ─▶ in_review ─▶ accepted ─▶ sold
//!      └──────────┴────────▶ rejected
//! ```

use serde::{Deserialize, Serialize};
use sqlx::types::time::OffsetDateTime;

use super::ids::{CompanyId, OsintReportId, UserId};
use super::report::ReportSeverity;

#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::Type, Serialize, Deserialize)]
#[sqlx(type_name = "osint_status", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum OsintStatus {
    Submitted,
    InReview,
    Accepted,
    Rejected,
    Sold,
}

impl OsintStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Submitted => "submitted",
            Self::InReview => "in_review",
            Self::Accepted => "accepted",
            Self::Rejected => "rejected",
            Self::Sold => "sold",
        }
    }
    pub fn from_str(s: &str) -> Option<Self> {
        Some(match s {
            "submitted" => Self::Submitted,
            "in_review" => Self::InReview,
            "accepted" => Self::Accepted,
            "rejected" => Self::Rejected,
            "sold" => Self::Sold,
            _ => return None,
        })
    }
    /// Etiqueta legible en español para la UI.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Submitted => "enviado",
            Self::InReview => "en revisión",
            Self::Accepted => "aceptado",
            Self::Rejected => "rechazado",
            Self::Sold => "vendido",
        }
    }
    /// El cuerpo completo del informe está disponible para una empresa solo
    /// cuando ya lo compró.
    pub const fn is_sold(self) -> bool {
        matches!(self, Self::Sold)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::Type, Serialize, Deserialize)]
#[sqlx(type_name = "osint_category", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum OsintCategory {
    ExposedCredentials,
    DataLeak,
    InfraExposure,
    BrandAbuse,
    DarkWeb,
    AttackSurface,
    SocialEngineering,
    Other,
}

impl OsintCategory {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ExposedCredentials => "exposed_credentials",
            Self::DataLeak => "data_leak",
            Self::InfraExposure => "infra_exposure",
            Self::BrandAbuse => "brand_abuse",
            Self::DarkWeb => "dark_web",
            Self::AttackSurface => "attack_surface",
            Self::SocialEngineering => "social_engineering",
            Self::Other => "other",
        }
    }
    pub fn from_str(s: &str) -> Option<Self> {
        Some(match s {
            "exposed_credentials" => Self::ExposedCredentials,
            "data_leak" => Self::DataLeak,
            "infra_exposure" => Self::InfraExposure,
            "brand_abuse" => Self::BrandAbuse,
            "dark_web" => Self::DarkWeb,
            "attack_surface" => Self::AttackSurface,
            "social_engineering" => Self::SocialEngineering,
            "other" => Self::Other,
            _ => return None,
        })
    }
    pub const fn label(self) -> &'static str {
        match self {
            Self::ExposedCredentials => "Credenciales expuestas",
            Self::DataLeak => "Fuga de datos",
            Self::InfraExposure => "Infraestructura expuesta",
            Self::BrandAbuse => "Abuso de marca",
            Self::DarkWeb => "Dark web",
            Self::AttackSurface => "Superficie de ataque",
            Self::SocialEngineering => "Ingeniería social",
            Self::Other => "Otro",
        }
    }
    /// Todas las categorías, para poblar selects.
    pub const fn all() -> [Self; 8] {
        [
            Self::ExposedCredentials,
            Self::DataLeak,
            Self::InfraExposure,
            Self::BrandAbuse,
            Self::DarkWeb,
            Self::AttackSurface,
            Self::SocialEngineering,
            Self::Other,
        ]
    }
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct OsintReportRecord {
    pub id: OsintReportId,
    pub public_id: String,
    pub researcher_id: UserId,
    pub subject_company_id: Option<CompanyId>,
    pub subject_name: String,
    pub title: String,
    pub category: OsintCategory,
    pub criticality: ReportSeverity,
    pub summary: String,
    pub body_md: String,
    pub price_cents: i32,
    pub resale_price_cents: Option<i32>,
    pub status: OsintStatus,
    pub reviewed_by: Option<UserId>,
    pub sold_to_company_id: Option<CompanyId>,
    pub sold_at: Option<OffsetDateTime>,
    pub created_at: OffsetDateTime,
}
