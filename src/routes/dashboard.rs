//! Dashboard del researcher. Resume su actividad (KPIs, action items,
//! reports recientes, payouts recientes, programas destacados). Alineado
//! con el flujo de HackerOne/Intigriti/Bugcrowd.

use axum::extract::State;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use time::format_description::well_known::Rfc3339;

use crate::auth::CurrentUser;
use crate::db;
use crate::domain::ids::UserId;
use crate::error::AppResult;
use crate::state::AppState;
use crate::web::templates::{
    DashboardAction, DashboardPayoutRow, DashboardProgramCard, DashboardReportRow,
    DashboardTemplate,
};

pub fn router() -> Router<AppState> {
    Router::new().route("/dashboard", get(index))
}

async fn index(
    State(state): State<AppState>,
    current: CurrentUser,
) -> AppResult<impl IntoResponse> {
    let user_id = UserId::from(current.user.id);
    let data = db::dashboard::load_for_researcher(&state.db, user_id).await?;

    let valid_rate = if data.kpi.reports_total > 0 {
        format!(
            "{}%",
            (data.kpi.reports_valid * 100 / data.kpi.reports_total).max(0)
        )
    } else {
        "—".into()
    };

    let rank_label = match data.kpi.rank_pct {
        Some(p) if p > 0.0 => format!("Top {:.0}%", p),
        _ => "—".into(),
    };

    Ok(DashboardTemplate {
        year: time::OffsetDateTime::now_utc().year(),
        handle: current.user.handle,
        role: current.user.role,
        kpi_reports_total: data.kpi.reports_total,
        kpi_reports_valid: data.kpi.reports_valid,
        kpi_valid_rate: valid_rate,
        kpi_bounties_total_usd: format_usd(data.kpi.bounties_total_cents),
        kpi_bounties_90d_usd: format_usd(data.kpi.bounties_90d_cents),
        kpi_reputation: data.kpi.reputation,
        kpi_rank_label: rank_label,
        action_items: data
            .action_items
            .into_iter()
            .map(|a| DashboardAction {
                kind: a.kind.to_string(),
                message: a.message,
                href: a.href,
            })
            .collect(),
        recent_reports: data
            .recent_reports
            .into_iter()
            .map(|r| DashboardReportRow {
                public_id: r.public_id,
                title: r.title,
                state: r.state,
                severity: r.severity,
                date: r.created_at.format(&Rfc3339).unwrap_or_default()[..10].to_string(),
            })
            .collect(),
        recent_payouts: data
            .recent_payouts
            .into_iter()
            .map(|p| DashboardPayoutRow {
                amount_usd: p.amount_usd,
                rail: p.rail,
                status: p.status,
                report_public_id: p.report_public_id,
                date: p.created_at.format(&Rfc3339).unwrap_or_default()[..10].to_string(),
            })
            .collect(),
        featured_programs: data
            .featured_programs
            .into_iter()
            .map(|p| DashboardProgramCard {
                href: format!("/programs/{}/{}", p.company_slug, p.program_slug),
                name: p.name,
                company_name: p.company_name,
                bounty_max_usd: p.bounty_max_usd,
            })
            .collect(),
    })
}

fn format_usd(cents: i64) -> String {
    format!("${:.2}", cents as f64 / 100.0)
}
