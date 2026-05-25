//! Landing pública en `/`. Hero + grid de programas públicos.
//! Sin auth — es el storefront del sitio.

use axum::extract::State;
use axum::response::IntoResponse;

use crate::db;
use crate::error::AppResult;
use crate::state::AppState;
use crate::web::shared::current_year;
use crate::web::templates::{HomeTemplate, ProgramPublicCardView};

pub async fn index(State(state): State<AppState>) -> AppResult<impl IntoResponse> {
    let rows = db::programs::list_public(&state.db).await?;
    let cards: Vec<ProgramPublicCardView> = rows
        .into_iter()
        .take(6) // hero: solo los 6 más recientes
        .map(|r| ProgramPublicCardView {
            company_slug: r.company_slug,
            company_name: r.company_name,
            program_slug: r.slug,
            name: r.name,
            summary: r.summary.unwrap_or_default(),
            bounty_low: r.bounty_low_cents.map(usd).unwrap_or_default(),
            bounty_critical: r.bounty_critical_cents.map(usd).unwrap_or_default(),
        })
        .collect();

    // IDs de logo en redseg.org/images/sN.jpg. El 14 no existe en el original.
    let base: Vec<i32> = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 15, 16, 17];
    let mut ally_ids = base.clone();
    ally_ids.extend(base); // duplicar para loop continuo del marquee

    Ok(HomeTemplate {
        year: current_year(),
        programs: cards,
        ally_ids,
    })
}

fn usd(cents: i32) -> String {
    format!("${}", cents / 100)
}
