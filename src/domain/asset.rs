//! Tipos de la entidad `assets`.
//!
//! El truco aquí es que `assets.target` es JSONB con shape distinto según
//! `asset_type`. Modelamos eso con un enum tagged por tipo, y el formulario
//! HTMX cambia los inputs según la variante seleccionada.

use serde::{Deserialize, Serialize};
use sqlx::types::time::OffsetDateTime;
use sqlx::types::JsonValue;

use super::ids::{AssetId, ProgramId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, sqlx::Type, Serialize, Deserialize)]
#[sqlx(type_name = "asset_type", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum AssetType {
    Web,
    Api,
    MobileAndroid,
    MobileIos,
    InfraHost,
    InfraRange,
    SourceRepo,
    Package,
    Firmware,
    HardwareDevice,
    IotEndpoint,
    RadioSignal,
    Other,
}

impl AssetType {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Web => "web",
            Self::Api => "api",
            Self::MobileAndroid => "mobile_android",
            Self::MobileIos => "mobile_ios",
            Self::InfraHost => "infra_host",
            Self::InfraRange => "infra_range",
            Self::SourceRepo => "source_repo",
            Self::Package => "package",
            Self::Firmware => "firmware",
            Self::HardwareDevice => "hardware_device",
            Self::IotEndpoint => "iot_endpoint",
            Self::RadioSignal => "radio_signal",
            Self::Other => "other",
        }
    }

    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Web => "Web app",
            Self::Api => "API",
            Self::MobileAndroid => "Android app",
            Self::MobileIos => "iOS app",
            Self::InfraHost => "Host de infraestructura",
            Self::InfraRange => "Rango CIDR",
            Self::SourceRepo => "Repositorio fuente",
            Self::Package => "Paquete",
            Self::Firmware => "Firmware",
            Self::HardwareDevice => "Dispositivo hardware",
            Self::IotEndpoint => "Endpoint IoT",
            Self::RadioSignal => "Señal de radio",
            Self::Other => "Otro",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        Some(match s {
            "web" => Self::Web,
            "api" => Self::Api,
            "mobile_android" => Self::MobileAndroid,
            "mobile_ios" => Self::MobileIos,
            "infra_host" => Self::InfraHost,
            "infra_range" => Self::InfraRange,
            "source_repo" => Self::SourceRepo,
            "package" => Self::Package,
            "firmware" => Self::Firmware,
            "hardware_device" => Self::HardwareDevice,
            "iot_endpoint" => Self::IotEndpoint,
            "radio_signal" => Self::RadioSignal,
            "other" => Self::Other,
            _ => return None,
        })
    }

    pub const fn all() -> [Self; 13] {
        [
            Self::Web, Self::Api, Self::MobileAndroid, Self::MobileIos,
            Self::InfraHost, Self::InfraRange, Self::SourceRepo, Self::Package,
            Self::Firmware, Self::HardwareDevice, Self::IotEndpoint,
            Self::RadioSignal, Self::Other,
        ]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::Type, Serialize, Deserialize)]
#[sqlx(type_name = "asset_severity_cap", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum AssetSeverityCap {
    Low,
    Medium,
    High,
    Critical,
    None,
}

impl AssetSeverityCap {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Critical => "critical",
            Self::None => "none",
        }
    }
    pub fn from_str(s: &str) -> Option<Self> {
        Some(match s {
            "low" => Self::Low, "medium" => Self::Medium, "high" => Self::High,
            "critical" => Self::Critical, "none" => Self::None, _ => return None,
        })
    }
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AssetRecord {
    pub id: AssetId,
    pub program_id: ProgramId,
    pub asset_type: AssetType,
    pub label: String,
    pub target: JsonValue,
    pub in_scope: bool,
    pub severity_cap: AssetSeverityCap,
    pub notes_md: Option<String>,
    pub created_at: OffsetDateTime,
}

/// Resumen del `target` para mostrar en listados (one-liner por tipo).
pub fn summarize_target(asset_type: AssetType, target: &JsonValue) -> String {
    let s = |k: &str| target.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
    match asset_type {
        AssetType::Web => s("url"),
        AssetType::Api => s("base_url"),
        AssetType::MobileAndroid => s("package"),
        AssetType::MobileIos => s("bundle_id"),
        AssetType::InfraHost => {
            let f = s("fqdn"); if !f.is_empty() { f } else { s("ipv4") }
        }
        AssetType::InfraRange => s("cidr"),
        AssetType::SourceRepo => s("url"),
        AssetType::Package => format!("{}:{}", s("ecosystem"), s("name")),
        AssetType::Firmware => format!("{} {}", s("vendor"), s("model")),
        AssetType::HardwareDevice => format!("{} {}", s("vendor"), s("model")),
        AssetType::IotEndpoint => format!("{} · {}", s("protocol"), s("endpoint")),
        AssetType::RadioSignal => format!("{} {}", s("band"), s("modulation")),
        AssetType::Other => s("description"),
    }
}
