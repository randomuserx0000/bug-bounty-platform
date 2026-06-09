# Iteración: Escudo Digital + producto OSINT

Resumen de lo construido en esta tanda de trabajo (rama mergeada a `main`).

## 1. Rebrand → "Escudo Digital"

- Wordmark `bugbounty.ve` → **Escudo Digital** en todos los templates, el topstrip,
  el footer, los `<title>` y el comentario de marca de `app.css` + README.
- **No** se renombró lo interno: crate/binario `bugbounty` (`Cargo.toml`), clave
  `localStorage('bb-theme')`, prefijo `VE-` de los public_id, ni el dominio de
  ejemplo en `DEPLOY.md`/`docker-compose.prod.yml` (es infraestructura).

## 2. Precios base de referencia

- `src/domain/pricing.rs` = **fuente única de verdad**: `OSINT_BASE_CENTS` ($50) y
  `SEVERITY_TIERS` (low $100–300, medium $400–600, high $700–900, critical $1.000–2.000).
- Se usan para **pre-rellenar el form de programa** (`templates/programs/new.html`,
  vía `web::shared::severity_tier_views()`).
- En la **home NO se muestran** (decisión de producto): la tabla de precios se quitó.

## 3. Producto OSINT (entidad separada)

Flujo: investigador vende → admin revisa/acepta (fija reventa) → empresa-objetivo
compra (debita su escrow). Margen (reventa − base) = ingreso de la plataforma.

- **Migración** `migrations/0004_osint.sql`: enums `osint_status`/`osint_category`,
  tabla `osint_reports`, secuencia `osint_public_seq` (IDs `OSINT-2026-00001`), y
  `payouts` extendido (`osint_report_id` + `report_id` nullable + CHECK) como prep.
- **Dominio** `src/domain/osint.rs` (+ `OsintReportId` en `ids.rs`).
- **DB** `src/db/osint.rs`: `create/find_by_public_id/list_for_researcher/
  list_for_review/list_catalog_for_company/accept/reject/purchase`. `purchase`
  debita escrow + marca `sold` en una transacción con compare-and-swap.
- **Rutas** `src/routes/osint.rs`: `/osint/new|mine|review|:id|:id/accept|:id/reject|
  :id/buy` + `/manage/:slug/osint` (catálogo). Gating del cuerpo: autor / admin /
  empresa compradora.
- **Templates** `templates/osint/*.html` + structs en `web/templates.rs` + nav en
  `base.html` y enlace en `companies/show.html`.
- **Auditoría**: `OSINT_CREATE/ACCEPT/REJECT/PURCHASE` en `audit/mod.rs`.

### Límite conocido (v1)

El **pago al investigador** se registra en `osint_reports.price_cents` y se notifica
por email; el desembolso real es operativo. La columna `payouts.osint_report_id`
queda lista para integrarlo en fase 2 (requiere cola de payouts a nivel plataforma
y `PayoutRecord.report_id` → `Option`).

## 4. Home rediseñada

- Quitado el rótulo "EL ARGUMENTO" y toda la sección de precios.
- Nueva sección **"Para hackers e investigadores — Tu talento, recompensado"**
  (reportar vulns, vender OSINT, formarse, cobrar) con CTAs a `/signup` y `/programs`.

## 5. Formación (OSINT Academy)

Solo **diseño documentado** en [osint-academy.md](osint-academy.md). Sin código.

## 6. Seguridad

Revisión en [security-review-osint.md](security-review-osint.md). Se endureció el
gating de `osint::show` para que la empresa-objetivo no vea informes no listados.
