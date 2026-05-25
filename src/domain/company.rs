//! Tipos de la entidad `companies` + tabla puente `company_members`.

use serde::{Deserialize, Serialize};
use sqlx::types::time::OffsetDateTime;

use super::ids::{CompanyId, UserId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::Type, Serialize, Deserialize)]
#[sqlx(type_name = "company_status", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum CompanyStatus {
    Pending,
    Active,
    Suspended,
}

impl CompanyStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Active => "active",
            Self::Suspended => "suspended",
        }
    }
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct CompanyRecord {
    pub id: CompanyId,
    pub slug: String,
    pub legal_name: String,
    pub display_name: String,
    pub country_code: Option<String>,
    pub website: Option<String>,
    pub description: Option<String>,
    pub status: CompanyStatus,
    pub escrow_balance_cents: i64,
    pub created_at: OffsetDateTime,
}

/// Rol del usuario dentro de una company. Mantenido como TEXT en SQL.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompanyRole {
    Owner,
    Admin,
    Triager,
    Member,
}

impl CompanyRole {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Owner => "owner",
            Self::Admin => "admin",
            Self::Triager => "triager",
            Self::Member => "member",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        Some(match s {
            "owner" => Self::Owner,
            "admin" => Self::Admin,
            "triager" => Self::Triager,
            "member" => Self::Member,
            _ => return None,
        })
    }

    /// Permisos para gestionar programs/assets dentro de la company.
    /// Owner y admin pueden todo; triager y member solo lectura/triaje
    /// (eso último lo verificarán los handlers de reports más adelante).
    pub const fn can_manage_programs(self) -> bool {
        matches!(self, Self::Owner | Self::Admin)
    }
}

#[derive(Debug, Clone)]
pub struct CompanyMembership {
    pub company_id: CompanyId,
    pub user_id: UserId,
    pub role: CompanyRole,
}
