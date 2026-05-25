//! Tipos del dominio (entidades + value objects).
//!
//! Aquí viven los structs que representan filas de la DB y los enums que
//! reflejan los `CREATE TYPE` del schema. Cada submódulo agrupa los tipos
//! de una entidad y NO contiene queries — las queries van en `db::`.

pub mod asset;
pub mod company;
pub mod ids;
pub mod payout;
pub mod program;
pub mod report;
// pub mod user;
