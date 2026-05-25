//! Enum `PaymentRail` + trait `Rail`.
//!
//! El enum mapea 1:1 al `CREATE TYPE payment_rail` de Postgres. Cualquier
//! variante nueva en SQL debe agregarse aquí y en una impl del trait.
//!
//! `Rail` es lo que la app llama: dado un rail y un blob de detalles, ¿es
//! válido? ¿cómo lo muestro? ¿cuánto cobra de fee? ¿está sujeto a OFAC?
//! De este modo los handlers no necesitan tener `match` sobre el enum.

use once_cell::sync::Lazy;
use regex::Regex;

use super::details::{BankUsdAccount, BankVesAccount, RailDetails};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, sqlx::Type, serde::Serialize, serde::Deserialize)]
#[sqlx(type_name = "payment_rail", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum PaymentRail {
    UsdtTrc20,
    UsdtErc20,
    Btc,
    BankUsd,
    BankVesSudeban,
    Paypal,
    BinancePay,
    Zinli,
}

impl PaymentRail {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::UsdtTrc20      => "usdt_trc20",
            Self::UsdtErc20      => "usdt_erc20",
            Self::Btc            => "btc",
            Self::BankUsd        => "bank_usd",
            Self::BankVesSudeban => "bank_ves_sudeban",
            Self::Paypal         => "paypal",
            Self::BinancePay     => "binance_pay",
            Self::Zinli          => "zinli",
        }
    }

    pub const fn display_name(self) -> &'static str {
        match self {
            Self::UsdtTrc20      => "USDT (TRC20)",
            Self::UsdtErc20      => "USDT (ERC20)",
            Self::Btc            => "Bitcoin",
            Self::BankUsd        => "Banco USD (SWIFT)",
            Self::BankVesSudeban => "Banco VES (SUDEBAN)",
            Self::Paypal         => "PayPal",
            Self::BinancePay     => "Binance Pay",
            Self::Zinli          => "Zinli",
        }
    }

    /// Devuelve el handler tipado para este rail.
    pub fn handler(self) -> Box<dyn Rail> {
        match self {
            Self::UsdtTrc20      => Box::new(UsdtTrc20),
            Self::UsdtErc20      => Box::new(UsdtErc20),
            Self::Btc            => Box::new(Btc),
            Self::BankUsd        => Box::new(BankUsd),
            Self::BankVesSudeban => Box::new(BankVesSudeban),
            Self::Paypal         => Box::new(Paypal),
            Self::BinancePay     => Box::new(BinancePay),
            Self::Zinli          => Box::new(Zinli),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RailError {
    #[error("detalle inválido: {0}")]
    Invalid(&'static str),
    #[error("formato no coincide con el rail")]
    WrongShape,
}

/// Operaciones comunes a todos los rails. Cada impl trabaja sobre
/// [`RailDetails`] y se queja si recibe la variante equivocada.
pub trait Rail: Send + Sync {
    /// Valida el shape + contenido (longitudes, checksums, regex).
    fn validate(&self, details: &RailDetails) -> Result<(), RailError>;

    /// Texto corto para listar en la UI (ej: "TRC...AB12").
    fn short_display(&self, details: &RailDetails) -> String;

    /// Fee estimado en USD cents que cobra la red/proveedor por una
    /// transferencia típica. Devuelve `None` si depende del monto/contexto.
    fn estimate_fee_cents(&self, amount_cents: i64) -> Option<i64>;

    /// `true` si este rail está sujeto a screening OFAC (cripto y banca
    /// internacional sí; rails locales VES no).
    fn requires_ofac_check(&self) -> bool;
}

// ============================================================================
// USDT TRC20 — la pieza prioritaria para VE
// ============================================================================

pub struct UsdtTrc20;

impl Rail for UsdtTrc20 {
    fn validate(&self, details: &RailDetails) -> Result<(), RailError> {
        let RailDetails::Crypto(c) = details else { return Err(RailError::WrongShape) };
        validate_tron_address(&c.address)
    }
    fn short_display(&self, details: &RailDetails) -> String {
        match details {
            RailDetails::Crypto(c) => mask_address(&c.address, 4, 4),
            _ => "?".into(),
        }
    }
    fn estimate_fee_cents(&self, _amount_cents: i64) -> Option<i64> {
        // Energy+bandwidth en TRON: ~1 USD por transfer USDT a address no activada,
        // ~0 si la address ya está activada (energía delegada). Conservador: 100c.
        Some(100)
    }
    fn requires_ofac_check(&self) -> bool { true }
}

// ============================================================================
// USDT ERC20
// ============================================================================

pub struct UsdtErc20;
impl Rail for UsdtErc20 {
    fn validate(&self, details: &RailDetails) -> Result<(), RailError> {
        let RailDetails::Crypto(c) = details else { return Err(RailError::WrongShape) };
        validate_eth_address(&c.address)
    }
    fn short_display(&self, details: &RailDetails) -> String {
        match details {
            RailDetails::Crypto(c) => mask_address(&c.address, 6, 4),
            _ => "?".into(),
        }
    }
    fn estimate_fee_cents(&self, _amount_cents: i64) -> Option<i64> { None }
    fn requires_ofac_check(&self) -> bool { true }
}

// ============================================================================
// BTC
// ============================================================================

pub struct Btc;
impl Rail for Btc {
    fn validate(&self, details: &RailDetails) -> Result<(), RailError> {
        let RailDetails::Crypto(c) = details else { return Err(RailError::WrongShape) };
        validate_btc_address(&c.address)
    }
    fn short_display(&self, details: &RailDetails) -> String {
        match details {
            RailDetails::Crypto(c) => mask_address(&c.address, 4, 4),
            _ => "?".into(),
        }
    }
    fn estimate_fee_cents(&self, _amount_cents: i64) -> Option<i64> { None }
    fn requires_ofac_check(&self) -> bool { true }
}

// ============================================================================
// Banco USD internacional (SWIFT/ACH)
// ============================================================================

pub struct BankUsd;
impl Rail for BankUsd {
    fn validate(&self, details: &RailDetails) -> Result<(), RailError> {
        let RailDetails::BankUsd(b) = details else { return Err(RailError::WrongShape) };
        validate_bank_usd(b)
    }
    fn short_display(&self, details: &RailDetails) -> String {
        match details {
            RailDetails::BankUsd(b) => format!("{} •••{}", b.bank_name, tail(&b.account_number, 4)),
            _ => "?".into(),
        }
    }
    fn estimate_fee_cents(&self, _amount_cents: i64) -> Option<i64> { Some(2500) } // ~$25 wire
    fn requires_ofac_check(&self) -> bool { true }
}

// ============================================================================
// Banco VES SUDEBAN — la otra pieza prioritaria para VE
// ============================================================================

pub struct BankVesSudeban;
impl Rail for BankVesSudeban {
    fn validate(&self, details: &RailDetails) -> Result<(), RailError> {
        let RailDetails::BankVes(b) = details else { return Err(RailError::WrongShape) };
        validate_bank_ves(b)
    }
    fn short_display(&self, details: &RailDetails) -> String {
        match details {
            RailDetails::BankVes(b) => format!("{} •••{}", b.bank_code, tail(&b.account_number, 4)),
            _ => "?".into(),
        }
    }
    fn estimate_fee_cents(&self, _amount_cents: i64) -> Option<i64> { Some(0) }
    fn requires_ofac_check(&self) -> bool { false }
}

// ============================================================================
// PayPal / Binance Pay / Zinli (PSPs con validación liviana)
// ============================================================================

pub struct Paypal;
impl Rail for Paypal {
    fn validate(&self, details: &RailDetails) -> Result<(), RailError> {
        let RailDetails::Email(e) = details else { return Err(RailError::WrongShape) };
        validate_email(&e.email)
    }
    fn short_display(&self, details: &RailDetails) -> String {
        match details { RailDetails::Email(e) => mask_email(&e.email), _ => "?".into() }
    }
    fn estimate_fee_cents(&self, amount_cents: i64) -> Option<i64> {
        Some((amount_cents * 5 / 100).max(50)) // ~5% friends&family no aplica a cross-border; conservador
    }
    fn requires_ofac_check(&self) -> bool { true }
}

pub struct BinancePay;
impl Rail for BinancePay {
    fn validate(&self, details: &RailDetails) -> Result<(), RailError> {
        let RailDetails::Handle(h) = details else { return Err(RailError::WrongShape) };
        validate_binance_pay_id(&h.handle)
    }
    fn short_display(&self, details: &RailDetails) -> String {
        match details { RailDetails::Handle(h) => h.handle.clone(), _ => "?".into() }
    }
    fn estimate_fee_cents(&self, _amount_cents: i64) -> Option<i64> { Some(0) }
    fn requires_ofac_check(&self) -> bool { true }
}

pub struct Zinli;
impl Rail for Zinli {
    fn validate(&self, details: &RailDetails) -> Result<(), RailError> {
        let RailDetails::Handle(h) = details else { return Err(RailError::WrongShape) };
        if h.handle.trim().is_empty() {
            return Err(RailError::Invalid("handle vacío"));
        }
        Ok(())
    }
    fn short_display(&self, details: &RailDetails) -> String {
        match details { RailDetails::Handle(h) => h.handle.clone(), _ => "?".into() }
    }
    fn estimate_fee_cents(&self, _amount_cents: i64) -> Option<i64> { Some(0) }
    fn requires_ofac_check(&self) -> bool { false }
}

// ============================================================================
// Validadores
// ============================================================================

fn validate_tron_address(addr: &str) -> Result<(), RailError> {
    // TRON mainnet: Base58Check, 34 chars, empieza con 'T', primer byte 0x41.
    if addr.len() != 34 || !addr.starts_with('T') {
        return Err(RailError::Invalid("TRON address debe tener 34 chars y empezar con T"));
    }
    let decoded = bs58::decode(addr)
        .with_check(None)
        .into_vec()
        .map_err(|_| RailError::Invalid("checksum Base58 inválido"))?;
    if decoded.len() != 21 || decoded[0] != 0x41 {
        return Err(RailError::Invalid("payload TRON inválido"));
    }
    Ok(())
}

fn validate_eth_address(addr: &str) -> Result<(), RailError> {
    static RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^0x[0-9a-fA-F]{40}$").expect("regex const"));
    if !RE.is_match(addr) {
        return Err(RailError::Invalid("ETH address debe ser 0x + 40 hex"));
    }
    Ok(())
}

fn validate_btc_address(addr: &str) -> Result<(), RailError> {
    // Sopa de formatos: P2PKH/P2SH (Base58Check) y bech32 (segwit/taproot).
    // Validación pragmática: longitud + prefijo. Validación de checksum exacta
    // requiere bech32 crate — lo dejamos pendiente para cuando habilitemos BTC.
    let l = addr.len();
    let ok_legacy = (26..=35).contains(&l) && (addr.starts_with('1') || addr.starts_with('3'));
    let ok_segwit = (42..=62).contains(&l) && (addr.starts_with("bc1") || addr.starts_with("tb1"));
    if ok_legacy || ok_segwit {
        Ok(())
    } else {
        Err(RailError::Invalid("BTC address no reconocida"))
    }
}

fn validate_bank_usd(b: &BankUsdAccount) -> Result<(), RailError> {
    if b.bank_name.trim().is_empty() {
        return Err(RailError::Invalid("bank_name vacío"));
    }
    if b.account_number.trim().is_empty() {
        return Err(RailError::Invalid("account_number vacío"));
    }
    if b.routing_or_swift.trim().is_empty() {
        return Err(RailError::Invalid("routing_or_swift vacío"));
    }
    if b.holder_name.trim().is_empty() {
        return Err(RailError::Invalid("holder_name vacío"));
    }
    if b.country_code.len() != 2 {
        return Err(RailError::Invalid("country_code debe ser ISO-2"));
    }
    Ok(())
}

fn validate_bank_ves(b: &BankVesAccount) -> Result<(), RailError> {
    if b.bank_code.len() != 4 || !b.bank_code.chars().all(|c| c.is_ascii_digit()) {
        return Err(RailError::Invalid("bank_code SUDEBAN debe ser 4 dígitos"));
    }
    if b.account_number.len() != 20 || !b.account_number.chars().all(|c| c.is_ascii_digit()) {
        return Err(RailError::Invalid("cuenta SUDEBAN debe ser 20 dígitos"));
    }
    // El número de cuenta SUDEBAN debe empezar con el bank_code.
    if !b.account_number.starts_with(&b.bank_code) {
        return Err(RailError::Invalid("la cuenta no coincide con el bank_code"));
    }
    if b.holder_name.trim().is_empty() {
        return Err(RailError::Invalid("holder_name vacío"));
    }
    validate_ve_holder_id(&b.holder_id)?;
    Ok(())
}

fn validate_ve_holder_id(id: &str) -> Result<(), RailError> {
    // V-XXXXXXXX (natural) o J-XXXXXXXX-X (jurídico/RIF).
    static RE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"^[VEJPG]-\d{7,9}(-\d)?$").expect("regex const"));
    if !RE.is_match(id) {
        return Err(RailError::Invalid("holder_id formato V-XXXXXXXX o J-XXXXXXXX-X"));
    }
    Ok(())
}

fn validate_email(email: &str) -> Result<(), RailError> {
    static RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r"^[A-Za-z0-9._%+\-]+@[A-Za-z0-9.\-]+\.[A-Za-z]{2,}$").expect("regex const")
    });
    if !RE.is_match(email) {
        return Err(RailError::Invalid("email inválido"));
    }
    Ok(())
}

fn validate_binance_pay_id(id: &str) -> Result<(), RailError> {
    // Binance Pay ID es numérico de 8-12 dígitos típicamente.
    if id.len() < 6 || id.len() > 16 || !id.chars().all(|c| c.is_ascii_digit()) {
        return Err(RailError::Invalid("Binance Pay ID debe ser 6-16 dígitos"));
    }
    Ok(())
}

// ============================================================================
// Helpers de presentación
// ============================================================================

fn mask_address(addr: &str, head: usize, tail: usize) -> String {
    if addr.len() <= head + tail + 1 {
        return addr.to_string();
    }
    format!("{}…{}", &addr[..head], &addr[addr.len() - tail..])
}

fn mask_email(email: &str) -> String {
    if let Some((user, dom)) = email.split_once('@') {
        let masked = if user.len() <= 2 {
            "•".repeat(user.len())
        } else {
            format!("{}•••{}", &user[..1], &user[user.len() - 1..])
        };
        format!("{masked}@{dom}")
    } else {
        email.to_string()
    }
}

fn tail(s: &str, n: usize) -> &str {
    if s.len() <= n { s } else { &s[s.len() - n..] }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tron_valid() {
        // Address conocida (ejemplo Tether contract). Solo chequeamos shape+checksum.
        assert!(validate_tron_address("TR7NHqjeKQxGTCi8q8ZY4pL8otSzgjLj6t").is_ok());
    }
    #[test]
    fn tron_bad_checksum() {
        assert!(validate_tron_address("TR7NHqjeKQxGTCi8q8ZY4pL8otSzgjLj6X").is_err());
    }
    #[test]
    fn tron_bad_prefix() {
        assert!(validate_tron_address("XR7NHqjeKQxGTCi8q8ZY4pL8otSzgjLj6t").is_err());
    }
    #[test]
    fn eth_ok() {
        assert!(validate_eth_address("0xdAC17F958D2ee523a2206206994597C13D831ec7").is_ok());
    }
    #[test]
    fn eth_bad() {
        assert!(validate_eth_address("0xZZZ").is_err());
    }
    #[test]
    fn ves_account_matches_bank_code() {
        let ok = BankVesAccount {
            bank_code: "0102".into(),
            account_number: "01020000000000000001".into(),
            holder_name: "Juan Pérez".into(),
            holder_id: "V-12345678".into(),
        };
        assert!(validate_bank_ves(&ok).is_ok());

        let bad = BankVesAccount {
            bank_code: "0102".into(),
            account_number: "01340000000000000001".into(),
            holder_name: "Juan".into(),
            holder_id: "V-12345678".into(),
        };
        assert!(validate_bank_ves(&bad).is_err());
    }
}
