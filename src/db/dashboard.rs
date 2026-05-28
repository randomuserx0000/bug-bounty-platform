//! Queries agregadas que alimentan el dashboard del researcher.
//!
//! Una sola función `load_for_researcher` para que el handler no tenga
//! que orquestar 5+ llamadas. Cada SELECT está hecho para devolver lo
//! mínimo que pinta la vista.

use sqlx::PgPool;
use time::OffsetDateTime;

use crate::domain::ids::UserId;

#[derive(Debug, Default)]
pub struct ResearcherDashboard {
    pub kpi: ResearcherKpi,
    pub action_items: Vec<ActionItem>,
    pub recent_reports: Vec<RecentReport>,
    pub recent_payouts: Vec<RecentPayout>,
    pub featured_programs: Vec<FeaturedProgram>,
}

#[derive(Debug, Default)]
pub struct ResearcherKpi {
    pub reports_total: i64,
    pub reports_valid: i64,        // accepted | resolved | disclosed
    pub bounties_total_cents: i64, // sum sent
    pub bounties_90d_cents: i64,
    pub reputation: i32,
    pub rank_pct: Option<f32>,
}

#[derive(Debug)]
pub struct ActionItem {
    pub kind: &'static str,    // "needs_info" | "payout_pending" | "no_payment_method"
    pub message: String,
    pub href: String,
}

#[derive(Debug)]
pub struct RecentReport {
    pub public_id: String,
    pub title: String,
    pub state: String,
    pub severity: String,
    pub created_at: OffsetDateTime,
}

#[derive(Debug)]
pub struct RecentPayout {
    pub amount_usd: String,
    pub rail: String,
    pub status: String,
    pub report_public_id: String,
    pub created_at: OffsetDateTime,
}

#[derive(Debug)]
pub struct FeaturedProgram {
    pub company_slug: String,
    pub program_slug: String,
    pub name: String,
    pub company_name: String,
    pub bounty_max_usd: String,
}

pub async fn load_for_researcher(
    pool: &PgPool,
    user_id: UserId,
) -> Result<ResearcherDashboard, sqlx::Error> {
    let kpi = load_kpi(pool, user_id).await?;
    let action_items = load_action_items(pool, user_id).await?;
    let recent_reports = load_recent_reports(pool, user_id).await?;
    let recent_payouts = load_recent_payouts(pool, user_id).await?;
    let featured_programs = load_featured_programs(pool).await?;
    Ok(ResearcherDashboard {
        kpi,
        action_items,
        recent_reports,
        recent_payouts,
        featured_programs,
    })
}

async fn load_kpi(pool: &PgPool, user_id: UserId) -> Result<ResearcherKpi, sqlx::Error> {
    // Conteos de reports en una sola pasada.
    let row: (i64, i64) = sqlx::query_as(
        "SELECT
            COUNT(*) FILTER (WHERE TRUE)                                  AS total,
            COUNT(*) FILTER (WHERE state IN ('accepted','resolved','disclosed')) AS valid
         FROM reports WHERE reporter_id = $1",
    )
    .bind(user_id)
    .fetch_one(pool)
    .await?;

    // Bounties pagados (solo payouts sent). 90d y total.
    let bounties: (Option<i64>, Option<i64>) = sqlx::query_as(
        "SELECT
            COALESCE(SUM(amount_cents) FILTER (WHERE status = 'sent'), 0)::BIGINT AS total,
            COALESCE(SUM(amount_cents) FILTER (
                WHERE status = 'sent' AND sent_at > now() - INTERVAL '90 days'
            ), 0)::BIGINT AS last_90d
         FROM payouts WHERE user_id = $1",
    )
    .bind(user_id)
    .fetch_one(pool)
    .await?;

    // researcher_stats (opcional — puede no existir aún).
    let stats: Option<(i32, Option<f32>)> = sqlx::query_as(
        "SELECT reputation, rank_pct FROM researcher_stats WHERE user_id = $1",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await?;

    let (reputation, rank_pct) = stats.unwrap_or((0, None));

    Ok(ResearcherKpi {
        reports_total: row.0,
        reports_valid: row.1,
        bounties_total_cents: bounties.0.unwrap_or(0),
        bounties_90d_cents: bounties.1.unwrap_or(0),
        reputation,
        rank_pct,
    })
}

async fn load_action_items(
    pool: &PgPool,
    user_id: UserId,
) -> Result<Vec<ActionItem>, sqlx::Error> {
    let mut items: Vec<ActionItem> = Vec::new();

    // 1) Reports en needs_info — el reporter tiene que responder.
    let needs_info: Vec<(String,)> = sqlx::query_as(
        "SELECT public_id FROM reports
         WHERE reporter_id = $1 AND state = 'needs_info'
         ORDER BY updated_at DESC LIMIT 5",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    for (pid,) in needs_info {
        items.push(ActionItem {
            kind: "needs_info",
            message: format!("Report {pid} espera información adicional"),
            href: format!("/reports/{pid}"),
        });
    }

    // 2) Payouts pending o failed — el researcher debería estar al tanto.
    let pending: Vec<(String,)> = sqlx::query_as(
        "SELECT p.id::text FROM payouts p
         WHERE p.user_id = $1 AND p.status IN ('pending','failed')
         ORDER BY p.created_at DESC LIMIT 5",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    if !pending.is_empty() {
        items.push(ActionItem {
            kind: "payout_pending",
            message: format!(
                "Tienes {} payout(s) en pending/failed",
                pending.len()
            ),
            href: "/payouts/mine".into(),
        });
    }

    // 3) ¿Tiene método de pago configurado?
    let pm_count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*)::BIGINT FROM payment_methods WHERE user_id = $1",
    )
    .bind(user_id)
    .fetch_one(pool)
    .await?;
    if pm_count.0 == 0 {
        items.push(ActionItem {
            kind: "no_payment_method",
            message: "Aún no has configurado un método de pago".into(),
            href: "/settings/payment-methods".into(),
        });
    }

    Ok(items)
}

async fn load_recent_reports(
    pool: &PgPool,
    user_id: UserId,
) -> Result<Vec<RecentReport>, sqlx::Error> {
    let rows: Vec<(String, String, String, String, OffsetDateTime)> = sqlx::query_as(
        "SELECT public_id, title, state::text, severity::text, created_at
         FROM reports
         WHERE reporter_id = $1
         ORDER BY created_at DESC LIMIT 5",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(public_id, title, state, severity, created_at)| RecentReport {
            public_id,
            title,
            state,
            severity,
            created_at,
        })
        .collect())
}

async fn load_recent_payouts(
    pool: &PgPool,
    user_id: UserId,
) -> Result<Vec<RecentPayout>, sqlx::Error> {
    let rows: Vec<(i64, String, String, String, OffsetDateTime)> = sqlx::query_as(
        "SELECT p.amount_cents, p.rail::text, p.status::text,
                r.public_id, p.created_at
         FROM payouts p
         JOIN reports r ON r.id = p.report_id
         WHERE p.user_id = $1
         ORDER BY p.created_at DESC LIMIT 5",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(amount_cents, rail, status, report_public_id, created_at)| RecentPayout {
            amount_usd: format!("${:.2}", amount_cents as f64 / 100.0),
            rail,
            status,
            report_public_id,
            created_at,
        })
        .collect())
}

async fn load_featured_programs(
    pool: &PgPool,
) -> Result<Vec<FeaturedProgram>, sqlx::Error> {
    let rows: Vec<(String, String, String, String, Option<i32>)> = sqlx::query_as(
        "SELECT c.slug, p.slug, p.name, c.display_name, p.bounty_critical_cents
         FROM programs p
         JOIN companies c ON c.id = p.company_id
         WHERE p.visibility = 'public' AND p.status = 'public'
         ORDER BY p.launched_at DESC NULLS LAST, p.created_at DESC
         LIMIT 4",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(
            |(company_slug, program_slug, name, company_name, bounty_max_cents)| {
                let bounty_max_usd = match bounty_max_cents {
                    Some(c) if c > 0 => format!("${:.0}", c as f64 / 100.0),
                    _ => "—".to_string(),
                };
                FeaturedProgram {
                    company_slug,
                    program_slug,
                    name,
                    company_name,
                    bounty_max_usd,
                }
            },
        )
        .collect())
}
