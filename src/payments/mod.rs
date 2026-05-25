//! Métodos de pago multi-rail.
//!
//! Este módulo modela las 8 vías por las que un researcher puede recibir un
//! payout en Venezuela: tres cripto (USDT TRC20/ERC20, BTC), dos bancarias
//! (USD internacional, VES vía SUDEBAN) y tres PSP (PayPal, Binance Pay,
//! Zinli). El enum vive sincronizado con el `payment_rail` de Postgres.
//!
//! El resto de la app habla con métodos de pago a través del trait
//! [`Rail`] — nadie debería hacer `match` sobre `PaymentRail` fuera de aquí.

pub mod crypto;
pub mod details;
pub mod rail;

pub use rail::{PaymentRail, Rail, RailError};
