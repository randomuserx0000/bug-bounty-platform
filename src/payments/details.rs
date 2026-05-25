//! Formas concretas del JSON de detalles por rail.
//!
//! El SQL documenta el shape, pero a nivel Rust queremos `serde_json`
//! tipado para no andar tocando `Value::get("address")` por todos lados.
//!
//! Para guardar: se serializa el enum a JSON y se cifra con
//! [`crate::payments::crypto::encrypt`]. Para leer: se descifra y se
//! deserializa según el rail almacenado.

use serde::{Deserialize, Serialize};

/// Datos cifrados específicos del rail, deserializados desde JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RailDetails {
    Crypto(CryptoAddress),
    BankUsd(BankUsdAccount),
    BankVes(BankVesAccount),
    Email(EmailHandle),
    Handle(HandleId),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CryptoAddress {
    pub address: String,
    /// Memo/tag opcional (algunas chains lo piden; TRON/ETH no).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memo: Option<String>,
}

/// Cuenta bancaria internacional en USD (típicamente vía SWIFT/ACH).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BankUsdAccount {
    pub bank_name: String,
    pub account_number: String,
    pub routing_or_swift: String,
    pub holder_name: String,
    pub country_code: String,
}

/// Cuenta bancaria venezolana SUDEBAN (20 dígitos).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BankVesAccount {
    pub bank_code: String,           // 4 dígitos (0102, 0134, ...)
    pub account_number: String,      // 20 dígitos
    pub holder_name: String,
    pub holder_id: String,           // V-12345678 / J-12345678-9
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailHandle {
    pub email: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandleId {
    pub handle: String,
}
