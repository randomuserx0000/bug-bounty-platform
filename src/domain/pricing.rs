//! Precios base de referencia (USD cents).
//!
//! Fuente única de verdad para: la home (tabla de precios), los defaults del
//! form de programa, y el producto OSINT. No es lógica de negocio rígida — las
//! empresas pueden fijar montos distintos por programa — sino la referencia que
//! comunica Escudo Digital.

/// Precio base que la plataforma paga al investigador por un informe OSINT
/// aceptado. En USD cents ($50).
pub const OSINT_BASE_CENTS: i32 = 5_000;

/// Precio de lanzamiento del curso "Analista de Ciberseguridad" (fundamentos
/// de ethical hacking, pentesting y OSINT). En USD cents ($25). Accesible a
/// propósito: el objetivo es captar y formar talento nuevo que luego venda
/// hallazgos en la plataforma, no monetizar el curso en sí.
pub const COURSE_ANALISTA_CENTS: i32 = 2_500;

/// Un tramo de precio por severidad, con su rango recomendado.
#[derive(Debug, Clone, Copy)]
pub struct Tier {
    /// Clave estable (coincide con `ReportSeverity::as_str`).
    pub key: &'static str,
    /// Etiqueta legible.
    pub label: &'static str,
    /// Emoji indicador (usado en la home).
    pub emoji: &'static str,
    pub min_cents: i32,
    pub max_cents: i32,
}

impl Tier {
    pub const fn min_usd(&self) -> i32 {
        self.min_cents / 100
    }
    pub const fn max_usd(&self) -> i32 {
        self.max_cents / 100
    }
    /// Rango formateado para mostrar: "$100 – $300".
    pub fn range_usd(&self) -> String {
        format!("${} – ${}", self.min_usd(), self.max_usd())
    }
}

/// Tramos recomendados por severidad de vulnerabilidad.
pub const SEVERITY_TIERS: [Tier; 4] = [
    Tier { key: "low",      label: "Low",      emoji: "🟢", min_cents: 10_000,  max_cents: 30_000 },
    Tier { key: "medium",   label: "Medium",   emoji: "🟡", min_cents: 40_000,  max_cents: 60_000 },
    Tier { key: "high",     label: "High",     emoji: "🟠", min_cents: 70_000,  max_cents: 90_000 },
    Tier { key: "critical", label: "Critical", emoji: "🔴", min_cents: 100_000, max_cents: 200_000 },
];
