# Revisión de seguridad — producto OSINT

Alcance: el código añadido en esta iteración (`src/routes/osint.rs`,
`src/db/osint.rs`, `src/domain/osint.rs`) y su interacción con auth/escrow.
Fecha: 2026-06-09.

## Controles verificados (OK)

| Área | Estado | Nota |
|---|---|---|
| **SQL injection** | ✅ | Todas las queries de `db::osint` usan `.bind()` parametrizado. |
| **XSS (stored)** | ✅ | `summary` y `body_md` se renderizan con `pulldown_cmark` + `ammonia::clean`. CSP global activa. |
| **Race / doble compra** | ✅ | `db::osint::purchase` debita escrow y hace `UPDATE … WHERE status='accepted'` (compare-and-swap) en una transacción; si otra compra ganó, `rows_affected==0` → `rollback` (revierte el débito). Sin doble cobro. |
| **Escrow insuficiente** | ✅ | El CHECK `escrow_balance_cents >= 0` rompe la tx; el handler lo traduce a error y no marca `sold`. |
| **Autorización (accept/reject)** | ✅ | `require_admin` (rol `admin`), server-side, idempotente vía `WHERE status IN (...)`. |
| **Autorización (buy)** | ✅ | Exige `status=accepted`, `subject_company_id` y membresía con `can_manage_programs` de ESA empresa. |
| **CSRF** | ✅ (baseline) | Cookie `bb_session` firmada + `HttpOnly` + `Secure` (prod) + `SameSite=Lax`; **todas las mutaciones son POST** → CSRF cross-site bloqueado. |
| **Enumeración** | ✅ | Accesos no autorizados → `404` (no se filtra existencia). |
| **Auditoría** | ✅ | `OSINT_CREATE/ACCEPT/REJECT/PURCHASE` registrados. |

## Hallazgos

### H-1 (corregido) — Fuga de informes no listados a la empresa-objetivo
`osint::show` permitía a un gestor de la empresa-objetivo ver el resumen de un
informe **antes de ser aceptado** (submitted/in_review/rejected), revelando su
existencia y el teaser. **Fix aplicado**: el sujeto solo puede ver el informe
cuando está `accepted`/`sold` (`subject_can_view = manages_subject && is_listed`).

### H-2 (corregido) — Sin rate-limit en envío de OSINT
`POST /osint` no tenía rate-limit. **Fix aplicado**: `GovernorLayer` por IP solo sobre
ese POST (ráfaga de 5, luego 1 cada 30s) vía un sub-router en `osint::router()`; las
vistas GET y el resto de acciones no se ven afectadas. Mejora futura posible: cuota
por usuario/día (el governor es por IP).

### H-3 (recomendación) — CSRF solo por `SameSite=Lax`
Es defensa suficiente para el MVP, pero no defensa en profundidad. **Sugerencia**:
tokens anti-CSRF en formularios mutadores si se quiere endurecer.

### H-4 (recomendación) — Auditar supply chain
El binario enlaza muchas crates estáticamente. **Sugerencia**: integrar en CI
`cargo audit` (CVEs RustSec) y `cargo geiger` (uso de `unsafe` en deps), que es el
vector de "bajo nivel" realista en una app Rust.

## Fuera de alcance / notas

- El desembolso al investigador (v1) es operativo; cuando se integre con `payouts`,
  revisar la misma lógica de escrow/race aplicada a ese flujo.
- Revisar periódicamente el gating de `reports` (`load_report_ctx`) con la misma
  lupa que se aplicó aquí.
