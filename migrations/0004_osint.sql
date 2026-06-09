-- Escudo Digital :: producto OSINT (entidad separada)
--
-- Investigadores envían informes OSINT sobre empresas; la plataforma los
-- revisa, los compra (precio base $50) y los revende a la empresa afiliada
-- para que remedie. El margen (reventa − base) es ingreso de la plataforma.

-- public_id tipo "OSINT-2026-00001". Contador global, prefijo año al insertar
-- (misma decisión que reports_public_seq).
CREATE SEQUENCE IF NOT EXISTS osint_public_seq START 1;

CREATE TYPE osint_status AS ENUM (
    'submitted',   -- enviado por el investigador
    'in_review',   -- un admin lo está revisando
    'accepted',    -- aceptado y comprado al investigador; en catálogo para la empresa
    'rejected',    -- descartado
    'sold'         -- una empresa lo compró (revente cerrada)
);

CREATE TYPE osint_category AS ENUM (
    'exposed_credentials',  -- credenciales filtradas
    'data_leak',            -- fuga de datos
    'infra_exposure',       -- infraestructura expuesta
    'brand_abuse',          -- abuso de marca / typosquatting
    'dark_web',             -- menciones en dark web
    'attack_surface',       -- superficie de ataque / shadow IT
    'social_engineering',   -- superficie de ingeniería social
    'other'
);

CREATE TABLE osint_reports (
    id                 UUID PRIMARY KEY,
    public_id          TEXT NOT NULL UNIQUE,
    researcher_id      UUID NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    -- Empresa objetivo. Si está afiliada a la plataforma, la enlazamos para
    -- ofrecerle la compra; si no, queda solo el nombre en texto libre.
    subject_company_id UUID REFERENCES companies(id) ON DELETE SET NULL,
    subject_name       TEXT NOT NULL,
    title              TEXT NOT NULL,
    category           osint_category NOT NULL,
    -- Reusamos el enum de severidad de reports para la criticidad del hallazgo.
    criticality        report_severity NOT NULL DEFAULT 'none',
    -- Teaser público (catálogo) vs. cuerpo completo (gated tras compra).
    summary            TEXT NOT NULL,
    body_md            TEXT NOT NULL,
    -- Lo que la plataforma paga al investigador (USD cents). Base $50.
    price_cents        INTEGER NOT NULL DEFAULT 5000 CHECK (price_cents >= 0),
    -- Lo que paga la empresa al comprar (lo fija el admin al aceptar).
    resale_price_cents INTEGER CHECK (resale_price_cents IS NULL OR resale_price_cents >= 0),
    status             osint_status NOT NULL DEFAULT 'submitted',
    reviewed_by        UUID REFERENCES users(id),
    sold_to_company_id UUID REFERENCES companies(id),
    sold_at            TIMESTAMPTZ,
    created_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at         TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_osint_researcher ON osint_reports(researcher_id);
CREATE INDEX idx_osint_status     ON osint_reports(status);
CREATE INDEX idx_osint_subject    ON osint_reports(subject_company_id);

CREATE TRIGGER trg_osint_updated BEFORE UPDATE ON osint_reports
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

-- ============================================================
-- PAYOUTS: soportar pago al investigador por un informe OSINT
-- ============================================================
-- Hasta ahora cada payout colgaba de un report de vulnerabilidad. Ahora un
-- payout puede originarse de un report O de un osint_report (exactamente uno).

ALTER TABLE payouts ALTER COLUMN report_id DROP NOT NULL;
ALTER TABLE payouts ADD COLUMN osint_report_id UUID REFERENCES osint_reports(id) ON DELETE RESTRICT;
ALTER TABLE payouts ADD CONSTRAINT chk_payout_source
    CHECK (num_nonnulls(report_id, osint_report_id) = 1);
CREATE INDEX idx_payouts_osint ON payouts(osint_report_id) WHERE osint_report_id IS NOT NULL;
