-- bugbounty-platform :: initial schema
-- Postgres 15+. UUIDv4 generated app-side (uuid crate).

CREATE EXTENSION IF NOT EXISTS citext;
CREATE EXTENSION IF NOT EXISTS pgcrypto;

-- ============================================================
-- ENUMS
-- ============================================================

CREATE TYPE user_role        AS ENUM ('researcher','company','admin','triager');
CREATE TYPE user_status      AS ENUM ('pending','active','suspended','banned');
CREATE TYPE kyc_status       AS ENUM ('none','submitted','verified','rejected');

CREATE TYPE company_status   AS ENUM ('pending','active','suspended');

CREATE TYPE program_status   AS ENUM ('draft','private','public','paused','closed');
CREATE TYPE program_visibility AS ENUM ('private','invite_only','public');

CREATE TYPE asset_type       AS ENUM (
    'web','api','mobile_android','mobile_ios',
    'infra_host','infra_range','source_repo','package',
    'firmware','hardware_device','iot_endpoint','radio_signal','other'
);
CREATE TYPE asset_severity_cap AS ENUM ('low','medium','high','critical','none');

CREATE TYPE report_state     AS ENUM (
    'new','triaging','needs_info','accepted','duplicate',
    'not_applicable','informative','resolved','disclosed','rejected'
);
CREATE TYPE report_severity  AS ENUM ('none','low','medium','high','critical');

CREATE TYPE payment_rail     AS ENUM (
    'usdt_trc20','usdt_erc20','btc',
    'bank_usd','bank_ves_sudeban',
    'paypal','binance_pay','zinli'
);
CREATE TYPE payout_status    AS ENUM ('pending','processing','sent','failed','reversed');

-- ============================================================
-- USERS
-- ============================================================

CREATE TABLE users (
    id              UUID PRIMARY KEY,
    email           CITEXT NOT NULL UNIQUE,
    handle          CITEXT NOT NULL UNIQUE,
    password_hash   TEXT NOT NULL,            -- argon2id
    role            user_role NOT NULL DEFAULT 'researcher',
    status          user_status NOT NULL DEFAULT 'pending',
    kyc_status      kyc_status NOT NULL DEFAULT 'none',
    -- Compliance: usuarios sancionados/OFAC.
    -- Decisión: NO bloquea registro, pero bloquea payouts a rails restringidos.
    ofac_flagged    BOOLEAN NOT NULL DEFAULT FALSE,
    country_code    CHAR(2),
    display_name    TEXT,
    bio             TEXT,
    avatar_url      TEXT,
    -- 2FA
    totp_secret     TEXT,
    totp_enabled    BOOLEAN NOT NULL DEFAULT FALSE,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_login_at   TIMESTAMPTZ
);

CREATE INDEX idx_users_role_status ON users(role, status);

CREATE TABLE user_sessions (
    id              UUID PRIMARY KEY,
    user_id         UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    token_hash      BYTEA NOT NULL UNIQUE,
    ip_inet         INET,
    user_agent      TEXT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at      TIMESTAMPTZ NOT NULL,
    revoked_at      TIMESTAMPTZ
);
CREATE INDEX idx_sessions_user ON user_sessions(user_id);
CREATE INDEX idx_sessions_expiry ON user_sessions(expires_at) WHERE revoked_at IS NULL;

-- Researcher reputation/stats (denormalized; recomputed via job)
CREATE TABLE researcher_stats (
    user_id         UUID PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    reputation      INTEGER NOT NULL DEFAULT 0,
    reports_total   INTEGER NOT NULL DEFAULT 0,
    reports_valid   INTEGER NOT NULL DEFAULT 0,
    bounties_paid_cents BIGINT NOT NULL DEFAULT 0,  -- normalizado a USD cents
    rank_pct        REAL,
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- ============================================================
-- COMPANIES
-- ============================================================

CREATE TABLE companies (
    id              UUID PRIMARY KEY,
    slug            CITEXT NOT NULL UNIQUE,
    legal_name      TEXT NOT NULL,
    display_name    TEXT NOT NULL,
    country_code    CHAR(2),
    website         TEXT,
    logo_url        TEXT,
    description     TEXT,
    status          company_status NOT NULL DEFAULT 'pending',
    -- Saldo prefondeado para pagos (escrow). En USD cents.
    escrow_balance_cents BIGINT NOT NULL DEFAULT 0 CHECK (escrow_balance_cents >= 0),
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE company_members (
    company_id      UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    user_id         UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role            TEXT NOT NULL DEFAULT 'member',  -- owner|admin|triager|member
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (company_id, user_id)
);

-- ============================================================
-- PROGRAMS
-- ============================================================

CREATE TABLE programs (
    id              UUID PRIMARY KEY,
    company_id      UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    slug            CITEXT NOT NULL,
    name            TEXT NOT NULL,
    summary         TEXT,
    policy_md       TEXT NOT NULL,        -- markdown: scope, rules, safe harbor
    visibility      program_visibility NOT NULL DEFAULT 'private',
    status          program_status NOT NULL DEFAULT 'draft',
    -- Rangos de bounty por severidad (USD cents). NULL = no aplica/desconocido.
    bounty_low_cents      INTEGER,
    bounty_medium_cents   INTEGER,
    bounty_high_cents     INTEGER,
    bounty_critical_cents INTEGER,
    -- Permite categorías especiales en este programa
    allows_redteam  BOOLEAN NOT NULL DEFAULT FALSE,
    allows_hardware BOOLEAN NOT NULL DEFAULT FALSE,
    -- Métricas (denormalizadas)
    response_efficiency_pct REAL,
    avg_triage_hours        REAL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    launched_at     TIMESTAMPTZ,
    UNIQUE (company_id, slug)
);

CREATE INDEX idx_programs_visibility_status ON programs(visibility, status);

-- Invitaciones a programas privados/invite_only
CREATE TABLE program_invites (
    program_id      UUID NOT NULL REFERENCES programs(id) ON DELETE CASCADE,
    user_id         UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    invited_by      UUID REFERENCES users(id),
    accepted_at     TIMESTAMPTZ,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (program_id, user_id)
);

-- ============================================================
-- ASSETS (polimórfico)
-- ============================================================
-- `target` JSONB: shape depende de asset_type
--   web:            { "url": "https://app.example.com", "scope": "*.example.com" }
--   api:            { "base_url": "...", "openapi": "..." }
--   mobile_android: { "package": "com.x.y", "min_version": "1.2.0", "sha256": "..." }
--   mobile_ios:     { "bundle_id": "com.x.y", "min_version": "1.2.0" }
--   infra_host:     { "fqdn": "host.x.com" } | { "ipv4": "1.2.3.4" }
--   infra_range:    { "cidr": "203.0.113.0/24" }
--   source_repo:    { "url": "https://github.com/...", "commit": "..." }
--   package:        { "ecosystem": "npm", "name": "...", "version": "..." }
--   firmware:       { "vendor": "...", "model": "...", "version": "...", "sha256": "..." }
--   hardware_device:{ "vendor": "...", "model": "...", "hw_rev": "...", "interfaces": ["uart","jtag"] }
--   iot_endpoint:   { "protocol": "mqtt|coap|...", "endpoint": "...", "model": "..." }
--   radio_signal:   { "band": "ism_915", "modulation": "...", "device": "..." }

CREATE TABLE assets (
    id              UUID PRIMARY KEY,
    program_id      UUID NOT NULL REFERENCES programs(id) ON DELETE CASCADE,
    asset_type      asset_type NOT NULL,
    label           TEXT NOT NULL,
    target          JSONB NOT NULL,
    in_scope        BOOLEAN NOT NULL DEFAULT TRUE,
    severity_cap    asset_severity_cap NOT NULL DEFAULT 'none',
    notes_md        TEXT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_assets_program ON assets(program_id);
CREATE INDEX idx_assets_type ON assets(asset_type);
CREATE INDEX idx_assets_target_gin ON assets USING GIN (target jsonb_path_ops);

-- ============================================================
-- REPORTS
-- ============================================================

CREATE TABLE reports (
    id              UUID PRIMARY KEY,
    public_id       TEXT NOT NULL UNIQUE,        -- short id tipo "VE-2026-00042"
    program_id      UUID NOT NULL REFERENCES programs(id) ON DELETE RESTRICT,
    asset_id        UUID REFERENCES assets(id) ON DELETE SET NULL,
    reporter_id     UUID NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    title           TEXT NOT NULL,
    description_md  TEXT NOT NULL,
    impact_md       TEXT,
    repro_md        TEXT,
    cwe             TEXT,                        -- p.ej. "CWE-79"
    cvss_vector     TEXT,                        -- vector string v3.1/v4
    cvss_score      NUMERIC(3,1),
    severity        report_severity NOT NULL DEFAULT 'none',
    state           report_state NOT NULL DEFAULT 'new',
    -- Triage
    assigned_to     UUID REFERENCES users(id),
    duplicate_of    UUID REFERENCES reports(id),
    bounty_amount_cents INTEGER,                 -- USD cents al cierre
    -- Disclosure
    disclosed_at    TIMESTAMPTZ,
    -- Timestamps de estado (para SLAs)
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    first_response_at TIMESTAMPTZ,
    triaged_at      TIMESTAMPTZ,
    resolved_at     TIMESTAMPTZ
);

CREATE INDEX idx_reports_program_state ON reports(program_id, state);
CREATE INDEX idx_reports_reporter ON reports(reporter_id);
CREATE INDEX idx_reports_state ON reports(state);
CREATE INDEX idx_reports_dup ON reports(duplicate_of) WHERE duplicate_of IS NOT NULL;

-- Adjuntos: PoCs, dumps, schematics, capturas SDR (.iq), pcaps, binarios
CREATE TABLE report_attachments (
    id              UUID PRIMARY KEY,
    report_id       UUID NOT NULL REFERENCES reports(id) ON DELETE CASCADE,
    uploader_id     UUID NOT NULL REFERENCES users(id),
    filename        TEXT NOT NULL,
    mime            TEXT NOT NULL,
    size_bytes      BIGINT NOT NULL CHECK (size_bytes >= 0),
    sha256          BYTEA NOT NULL,
    storage_key     TEXT NOT NULL,           -- key en object storage (S3/Hetzner/etc.)
    kind            TEXT NOT NULL,           -- 'poc'|'firmware'|'pcap'|'schematic'|'sdr_iq'|'video'|'other'
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_attachments_report ON report_attachments(report_id);
CREATE INDEX idx_attachments_sha ON report_attachments(sha256);

-- Comentarios e historial de estado
CREATE TABLE report_events (
    id              UUID PRIMARY KEY,
    report_id       UUID NOT NULL REFERENCES reports(id) ON DELETE CASCADE,
    actor_id        UUID REFERENCES users(id),
    event_type      TEXT NOT NULL,           -- 'comment'|'state_change'|'severity_change'|'assign'|'bounty_set'|'system'
    body_md         TEXT,
    metadata        JSONB,                   -- p.ej. {"from":"new","to":"triaging"}
    is_internal     BOOLEAN NOT NULL DEFAULT FALSE,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_events_report_created ON report_events(report_id, created_at);

-- ============================================================
-- PAGOS (multi-rail)
-- ============================================================

CREATE TABLE payment_methods (
    id              UUID PRIMARY KEY,
    user_id         UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    rail            payment_rail NOT NULL,
    label           TEXT,
    -- Datos específicos del rail (cifrados a nivel app antes de guardar)
    -- USDT/BTC: { "address": "...", "memo": "..." }
    -- bank_*:   { "bank":"...","account":"...","holder":"...","rif":"..." }
    -- paypal:   { "email": "..." }
    -- zinli:    { "handle": "..." }
    -- binance_pay: { "pay_id": "..." }
    details_enc     BYTEA NOT NULL,
    is_default      BOOLEAN NOT NULL DEFAULT FALSE,
    verified_at     TIMESTAMPTZ,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_pm_user ON payment_methods(user_id);
CREATE UNIQUE INDEX uq_pm_user_default
    ON payment_methods(user_id) WHERE is_default = TRUE;

CREATE TABLE payouts (
    id              UUID PRIMARY KEY,
    report_id       UUID NOT NULL REFERENCES reports(id) ON DELETE RESTRICT,
    company_id      UUID NOT NULL REFERENCES companies(id) ON DELETE RESTRICT,
    user_id         UUID NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    payment_method_id UUID REFERENCES payment_methods(id),
    rail            payment_rail NOT NULL,
    amount_cents    BIGINT NOT NULL CHECK (amount_cents > 0),    -- normalizado USD cents
    rail_amount     NUMERIC(38,18),                              -- monto en moneda nativa del rail
    rail_currency   TEXT,                                        -- "USDT","BTC","VES","USD"
    fx_rate         NUMERIC(38,18),                              -- USD -> rail_currency
    fee_cents       BIGINT NOT NULL DEFAULT 0,
    status          payout_status NOT NULL DEFAULT 'pending',
    tx_ref          TEXT,                                        -- txid blockchain / referencia banca / id PSP
    error_message   TEXT,
    -- Compliance gate
    ofac_check_at   TIMESTAMPTZ,
    ofac_passed     BOOLEAN,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    sent_at         TIMESTAMPTZ,
    confirmed_at    TIMESTAMPTZ
);
CREATE INDEX idx_payouts_user ON payouts(user_id);
CREATE INDEX idx_payouts_company ON payouts(company_id);
CREATE INDEX idx_payouts_status ON payouts(status);
CREATE INDEX idx_payouts_report ON payouts(report_id);

-- ============================================================
-- AUDIT LOG (global, append-only)
-- ============================================================

CREATE TABLE audit_log (
    id              BIGSERIAL PRIMARY KEY,
    actor_id        UUID REFERENCES users(id),
    actor_ip        INET,
    action          TEXT NOT NULL,            -- 'user.login'|'report.state_change'|'payout.send'|...
    target_type     TEXT,                     -- 'user'|'report'|'program'|...
    target_id       TEXT,
    metadata        JSONB,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_audit_created ON audit_log(created_at DESC);
CREATE INDEX idx_audit_actor ON audit_log(actor_id, created_at DESC);
CREATE INDEX idx_audit_target ON audit_log(target_type, target_id);

-- ============================================================
-- TRIGGERS: updated_at automático
-- ============================================================

CREATE OR REPLACE FUNCTION set_updated_at() RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = now();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_users_updated      BEFORE UPDATE ON users
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();
CREATE TRIGGER trg_companies_updated  BEFORE UPDATE ON companies
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();
CREATE TRIGGER trg_programs_updated   BEFORE UPDATE ON programs
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();
CREATE TRIGGER trg_assets_updated     BEFORE UPDATE ON assets
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();
CREATE TRIGGER trg_reports_updated    BEFORE UPDATE ON reports
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();
