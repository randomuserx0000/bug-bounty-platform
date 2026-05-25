//! Tipos de la entidad `payouts`.
//!
//! State machine simple: pending → processing → sent | failed | reversed.
//! En el MVP solo usamos pending/sent/failed; processing entra cuando
//! tengamos integración real con cada rail.

use serde::{Deserialize, Serialize};
use sqlx::types::time::OffsetDateTime;

use super::ids::{CompanyId, PaymentMethodId, PayoutId, ReportId, UserId};
use crate::payments::PaymentRail;

#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::Type, Serialize, Deserialize)]
#[sqlx(type_name = "payout_status", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum PayoutStatus {
    Pending,
    Processing,
    Sent,
    Failed,
    Reversed,
}

impl PayoutStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Processing => "processing",
            Self::Sent => "sent",
            Self::Failed => "failed",
            Self::Reversed => "reversed",
        }
    }
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PayoutRecord {
    pub id: PayoutId,
    pub report_id: ReportId,
    pub company_id: CompanyId,
    pub user_id: UserId,
    pub payment_method_id: Option<PaymentMethodId>,
    pub rail: PaymentRail,
    pub amount_cents: i64,
    pub fee_cents: i64,
    pub status: PayoutStatus,
    pub tx_ref: Option<String>,
    pub error_message: Option<String>,
    pub created_at: OffsetDateTime,
    pub sent_at: Option<OffsetDateTime>,
}
