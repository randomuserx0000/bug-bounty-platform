//! Tipos de la entidad `reports` + state machine + permisos.
//!
//! El workflow es largo (10 estados) pero las transiciones válidas son
//! pocas: triagers/owners mueven adelante, researchers piden info o
//! disputan. Validamos en código para no depender solo de la UI.

use serde::{Deserialize, Serialize};
use sqlx::types::time::OffsetDateTime;

use super::ids::{AssetId, ProgramId, ReportId, UserId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::Type, Serialize, Deserialize)]
#[sqlx(type_name = "report_state", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum ReportState {
    New,
    Triaging,
    NeedsInfo,
    Accepted,
    Duplicate,
    NotApplicable,
    Informative,
    Resolved,
    Disclosed,
    Rejected,
}

impl ReportState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::New => "new",
            Self::Triaging => "triaging",
            Self::NeedsInfo => "needs_info",
            Self::Accepted => "accepted",
            Self::Duplicate => "duplicate",
            Self::NotApplicable => "not_applicable",
            Self::Informative => "informative",
            Self::Resolved => "resolved",
            Self::Disclosed => "disclosed",
            Self::Rejected => "rejected",
        }
    }
    pub fn from_str(s: &str) -> Option<Self> {
        Some(match s {
            "new" => Self::New, "triaging" => Self::Triaging, "needs_info" => Self::NeedsInfo,
            "accepted" => Self::Accepted, "duplicate" => Self::Duplicate,
            "not_applicable" => Self::NotApplicable, "informative" => Self::Informative,
            "resolved" => Self::Resolved, "disclosed" => Self::Disclosed,
            "rejected" => Self::Rejected, _ => return None,
        })
    }

    /// Estados terminales (no admiten más transiciones).
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Disclosed | Self::Rejected)
    }

    /// Transiciones válidas. Lista exhaustiva por seguridad — cualquier
    /// transición no listada se rechaza en el handler.
    ///
    /// Reglas resumen:
    /// - `new` solo avanza a triaging (cuando el triager lo abre) o
    ///   needs_info (si el reporter quiere agregar info antes del triage).
    /// - `triaging` se ramifica al outcome del análisis.
    /// - `accepted` típicamente avanza a resolved cuando se paga/parchea.
    /// - `resolved` puede ir a disclosed (publicar) o ser definitivo.
    /// - Cualquier estado puede ir a `rejected` si se cierra como inválido.
    ///
    /// La verificación de quién PUEDE hacer la transición está aparte
    /// en [`can_actor_transition`].
    pub fn can_transition_to(self, target: Self) -> bool {
        use ReportState::*;
        if self == target { return false; }
        match (self, target) {
            (New, Triaging | NeedsInfo | Rejected) => true,
            (Triaging, NeedsInfo | Accepted | Duplicate | NotApplicable | Informative | Rejected) => true,
            (NeedsInfo, Triaging | Rejected) => true,
            (Accepted, Resolved | Rejected) => true,
            (Duplicate | NotApplicable | Informative, Disclosed | Rejected) => true,
            (Resolved, Disclosed) => true,
            _ => false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::Type, Serialize, Deserialize)]
#[sqlx(type_name = "report_severity", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum ReportSeverity {
    None,
    Low,
    Medium,
    High,
    Critical,
}

impl ReportSeverity {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none", Self::Low => "low", Self::Medium => "medium",
            Self::High => "high", Self::Critical => "critical",
        }
    }
    pub fn from_str(s: &str) -> Option<Self> {
        Some(match s {
            "none" => Self::None, "low" => Self::Low, "medium" => Self::Medium,
            "high" => Self::High, "critical" => Self::Critical, _ => return None,
        })
    }
}

/// Quién puede ejecutar la transición. El reporter solo puede cerrar como
/// `rejected` (retirar su propio reporte) o pasar a/desde `needs_info`. Lo
/// demás es del triage side.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActorKind {
    Reporter,
    Triager, // member de la company del programa con can_manage_programs
}

pub fn can_actor_transition(actor: ActorKind, from: ReportState, to: ReportState) -> bool {
    if !from.can_transition_to(to) { return false; }
    use ActorKind::*;
    use ReportState::*;
    match actor {
        Triager => true, // los triagers pueden cualquier transición válida
        Reporter => matches!(
            (from, to),
            (NeedsInfo, Triaging)   // responde al pedido de info
            | (New, NeedsInfo)      // pide pausa para agregar contexto
            | (New | Triaging | NeedsInfo, Rejected) // retira su propio reporte
        ),
    }
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ReportRecord {
    pub id: ReportId,
    pub public_id: String,
    pub program_id: ProgramId,
    pub asset_id: Option<AssetId>,
    pub reporter_id: UserId,
    pub title: String,
    pub description_md: String,
    pub impact_md: Option<String>,
    pub repro_md: Option<String>,
    pub cwe: Option<String>,
    pub cvss_vector: Option<String>,
    /// `cvss_score` no se carga en el MVP — se ignora vía `COALESCE(NULL)`
    /// para no requerir feature de `bigdecimal` en sqlx. Se exponen vector
    /// + severity manual.
    pub severity: ReportSeverity,
    pub state: ReportState,
    pub assigned_to: Option<UserId>,
    pub bounty_amount_cents: Option<i32>,
    pub created_at: OffsetDateTime,
}

/// Evento en la timeline del report. `event_type` es TEXT en SQL pero acá
/// modelamos los tipos conocidos. Body en markdown para los `comment`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventType {
    Comment,
    StateChange,
    SeverityChange,
    Assign,
    BountySet,
    System,
}

impl EventType {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Comment => "comment",
            Self::StateChange => "state_change",
            Self::SeverityChange => "severity_change",
            Self::Assign => "assign",
            Self::BountySet => "bounty_set",
            Self::System => "system",
        }
    }
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ReportEventRecord {
    pub id: super::ids::ReportEventId,
    pub report_id: ReportId,
    pub actor_id: Option<UserId>,
    pub event_type: String,
    pub body_md: Option<String>,
    pub metadata: Option<sqlx::types::JsonValue>,
    pub is_internal: bool,
    pub created_at: OffsetDateTime,
}
