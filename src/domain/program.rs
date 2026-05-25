//! Tipos de la entidad `programs`.

use serde::{Deserialize, Serialize};
use sqlx::types::time::OffsetDateTime;

use super::ids::{CompanyId, ProgramId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::Type, Serialize, Deserialize)]
#[sqlx(type_name = "program_visibility", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum ProgramVisibility {
    Private,
    InviteOnly,
    Public,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::Type, Serialize, Deserialize)]
#[sqlx(type_name = "program_status", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum ProgramStatus {
    Draft,
    Private,
    Public,
    Paused,
    Closed,
}

impl ProgramVisibility {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Private => "private",
            Self::InviteOnly => "invite_only",
            Self::Public => "public",
        }
    }
}

impl ProgramStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Private => "private",
            Self::Public => "public",
            Self::Paused => "paused",
            Self::Closed => "closed",
        }
    }
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ProgramRecord {
    pub id: ProgramId,
    pub company_id: CompanyId,
    pub slug: String,
    pub name: String,
    pub summary: Option<String>,
    pub policy_md: String,
    pub visibility: ProgramVisibility,
    pub status: ProgramStatus,
    pub bounty_low_cents: Option<i32>,
    pub bounty_medium_cents: Option<i32>,
    pub bounty_high_cents: Option<i32>,
    pub bounty_critical_cents: Option<i32>,
    pub allows_redteam: bool,
    pub allows_hardware: bool,
    pub created_at: OffsetDateTime,
    pub launched_at: Option<OffsetDateTime>,
}
