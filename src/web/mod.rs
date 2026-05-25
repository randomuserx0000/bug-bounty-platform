//! Capa de presentación: templates Askama + helpers HTML.
//!
//! Cada template `.html` tiene su struct acompañante con el mismo nombre
//! anotado con `#[derive(Template)]`. Los campos del struct son las
//! variables disponibles en el template. Askama valida la correspondencia
//! en tiempo de compilación.

pub mod shared;
pub mod templates;
