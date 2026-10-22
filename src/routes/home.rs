//! Landing pública en `/`. Hero + grid de programas públicos.
//! Sin auth — es el storefront del sitio.

use axum::extract::State;
use axum::response::IntoResponse;

use crate::auth::MaybeUser;
use crate::db;
use crate::error::AppResult;
use crate::state::AppState;
use crate::web::shared::{current_year, program_card};
use crate::web::templates::{HomeTemplate, ProgramPublicCardView};

pub async fn index(
    State(state): State<AppState>,
    MaybeUser(user): MaybeUser,
) -> AppResult<impl IntoResponse> {
    let rows = db::programs::list_public(&state.db).await?;
    let cards: Vec<ProgramPublicCardView> = rows
        .into_iter()
        .take(6) // hero: solo los 6 más recientes
        .map(program_card)
        .collect();

    // IDs de logo en redseg.org/images/sN.jpg. El 14 no existe en el original.
    let base: Vec<i32> = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 15, 16, 17];
    let mut ally_ids = base.clone();
    ally_ids.extend(base); // duplicar para loop continuo del marquee

    Ok(HomeTemplate {
        year: current_year(),
        programs: cards,
        ally_ids,
        account_role: user.as_ref().map(|u| u.role.clone()).unwrap_or_default(),
        handle: user.map(|u| u.handle).unwrap_or_default(),
    })
}


