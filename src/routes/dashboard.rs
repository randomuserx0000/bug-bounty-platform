//! Dashboard post-login, ramificado por rol:
//! - **researcher** → resumen de caza (KPIs, action items, reports, payouts, programas).
//! - **company** (o staff) → sus empresas con escrow, programas y payouts pendientes.
//! - **admin** → panel de plataforma (revisión OSINT, audit).

use axum::extract::State;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use time::format_description::well_known::Rfc3339;

use crate::auth::CurrentUser;
use crate::db;
use crate::domain::ids::UserId;
use crate::error::AppResult;
use crate::state::AppState;
use crate::web::templates::{
    AdminDashboardTemplate, CompanyDashCard, CompanyDashboardTemplate, DashboardAction,
    DashboardPayoutRow, DashboardProgramCard, DashboardReportRow, DashboardTemplate,
};

pub fn router() -> Router<AppState> {
    Router::new().route("/dashboard", get(index))
}

async fn index(State(state): State<AppState>, current: CurrentUser) -> AppResult<Response> {
    match current.user.role.as_str() {
        "company" | "triager" => company_dashboard(&state, current).await,
        "admin" => admin_dashboard(&state, current).await,
        _ => researcher_dashboard(&state, current).await,
    }
}

// ----------------------------------------------------------------------------
// Researcher
// ----------------------------------------------------------------------------

async fn researcher_dashboard(state: &AppState, current: CurrentUser) -> AppResult<Response> {
    let user_id = UserId::from(current.user.id);
    let data = db::dashboard::load_for_researcher(&state.db, user_id).await?;

    let valid_rate = if data.kpi.reports_total > 0 {
        format!("{}%", (data.kpi.reports_valid * 100 / data.kpi.reports_total).max(0))
    } else {
        "—".into()
    };
    let rank_label = match data.kpi.rank_pct {
        Some(p) if p > 0.0 => format!("Top {:.0}%", p),
        _ => "—".into(),
    };

    Ok(DashboardTemplate {
        year: time::OffsetDateTime::now_utc().year(),
        account_role: current.user.role.clone(),
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
            .map(|a| DashboardAction { kind: a.kind.to_string(), message: a.message, href: a.href })
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
    }
    .into_response())
}

// ----------------------------------------------------------------------------
// Company
// ----------------------------------------------------------------------------

async fn company_dashboard(state: &AppState, current: CurrentUser) -> AppResult<Response> {
    use crate::domain::payout::PayoutStatus;
    let user_id = UserId::from(current.user.id);
    let companies = db::companies::list_for_user(&state.db, user_id).await?;

    let mut cards = Vec::with_capacity(companies.len());
    for (c, role) in companies {
        let (escrow, programs, payouts, pending_reports) = tokio::join!(
            db::companies::escrow_balance(&state.db, c.id),
            db::programs::list_for_company(&state.db, c.id),
            db::payouts::list_for_company(&state.db, c.id),
            db::reports::count_pending_for_company(&state.db, c.id),
        );
        let escrow = escrow.unwrap_or(0);
        let programs_count = programs.map(|p| p.len()).unwrap_or(0);
        let pending_payouts = payouts
            .map(|ps| {
                ps.iter()
                    .filter(|p| {
                        matches!(
                            p.status,
                            PayoutStatus::Pending | PayoutStatus::Processing | PayoutStatus::Failed
                        )
                    })
                    .count()
            })
            .unwrap_or(0);
        let pending_reports = pending_reports.unwrap_or(0);
        cards.push(CompanyDashCard {
            slug: c.slug,
            name: c.display_name,
            role: role.as_str().into(),
            escrow_usd: format_usd(escrow),
            programs_count,
            pending_payouts,
            pending_reports,
        });
    }

    Ok(CompanyDashboardTemplate {
        year: time::OffsetDateTime::now_utc().year(),
        account_role: current.user.role.clone(),
        handle: current.user.handle,
        companies: cards,
    }
    .into_response())
}

// ----------------------------------------------------------------------------
// Admin
// ----------------------------------------------------------------------------

async fn admin_dashboard(state: &AppState, current: CurrentUser) -> AppResult<Response> {
    let osint_pending = db::osint::list_for_review(&state.db)
        .await
        .map(|v| v.len())
        .unwrap_or(0);

    Ok(AdminDashboardTemplate {
        year: time::OffsetDateTime::now_utc().year(),
        account_role: current.user.role.clone(),
        handle: current.user.handle,
        osint_pending,
    }
    .into_response())
}

fn format_usd(cents: i64) -> String {
    format!("${:.2}", cents as f64 / 100.0)
}
