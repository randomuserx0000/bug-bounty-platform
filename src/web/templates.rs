//! Structs Askama. Uno por template.
//!
//! Convención: `XxxTemplate` para páginas completas, `XxxPartial` para
//! fragmentos que devuelve HTMX.

use askama::Template;

#[derive(Template)]
#[template(path = "login.html")]
pub struct LoginTemplate {
    pub year: i32,
    /// URL relativa interna a la que volver tras login exitoso.
    pub next: String,
    /// Si la app tiene credenciales de Google configuradas, mostrar el botón.
    pub google_enabled: bool,
    /// Handle del usuario logueado (vacío = no logueado). Usado por base.html
    /// para decidir si pintar el menú de usuario o los CTA Entrar/Registrarme.
    pub handle: String,
    pub account_role: String,
}

#[derive(Template)]
#[template(path = "dashboard.html")]
pub struct DashboardTemplate {
    pub year: i32,
    pub handle: String,
    pub account_role: String,
    pub role: String,
    // KPIs (todos pre-formateados como string para no calcular en el template)
    pub kpi_reports_total: i64,
    pub kpi_reports_valid: i64,
    pub kpi_valid_rate: String,        // "62%" o "—"
    pub kpi_bounties_total_usd: String, // "$1,234.00"
    pub kpi_bounties_90d_usd: String,
    pub kpi_reputation: i32,
    pub kpi_rank_label: String,        // "Top 5%" o "—"
    // Action items / feed / lists
    pub action_items: Vec<DashboardAction>,
    pub recent_reports: Vec<DashboardReportRow>,
    pub recent_payouts: Vec<DashboardPayoutRow>,
    pub featured_programs: Vec<DashboardProgramCard>,
}

pub struct DashboardAction {
    pub kind: String,
    pub message: String,
    pub href: String,
}

/// Tarjeta de empresa en el dashboard de cuentas tipo company.
pub struct CompanyDashCard {
    pub slug: String,
    pub name: String,
    pub role: String,
    pub escrow_usd: String,
    pub programs_count: usize,
    pub pending_payouts: usize,
}

#[derive(Template)]
#[template(path = "dashboard_company.html")]
pub struct CompanyDashboardTemplate {
    pub year: i32,
    pub handle: String,
    pub account_role: String,
    pub companies: Vec<CompanyDashCard>,
}

#[derive(Template)]
#[template(path = "dashboard_admin.html")]
pub struct AdminDashboardTemplate {
    pub year: i32,
    pub handle: String,
    pub account_role: String,
    /// Informes OSINT pendientes de revisión.
    pub osint_pending: usize,
}

pub struct DashboardReportRow {
    pub public_id: String,
    pub title: String,
    pub state: String,
    pub severity: String,
    pub date: String,
}

pub struct DashboardPayoutRow {
    pub amount_usd: String,
    pub rail: String,
    pub status: String,
    pub report_public_id: String,
    pub date: String,
}

pub struct DashboardProgramCard {
    pub href: String,
    pub name: String,
    pub company_name: String,
    pub bounty_max_usd: String,
}

#[derive(Template)]
#[template(path = "signup.html")]
pub struct SignupTemplate {
    pub year: i32,
    pub google_enabled: bool,
    pub handle: String,
    pub account_role: String,
}

/// Fragmento de error de formulario. Lo inyecta HTMX en `#form-feedback`.
#[derive(Template)]
#[template(path = "partials/form_error.html")]
pub struct FormErrorPartial {
    pub message: String,
}

/// Fragmento de éxito de formulario. Lo inyecta HTMX en `#form-feedback`.
#[derive(Template)]
#[template(path = "partials/form_ok.html")]
pub struct FormOkPartial {
    pub message: String,
}

// ---------- cursos / academia ----------

#[derive(Template)]
#[template(path = "courses/index.html")]
pub struct CourseCatalogTemplate {
    pub year: i32,
    pub handle: String,
    pub account_role: String,
    pub price_analista: i32,
}

#[derive(Template)]
#[template(path = "courses/analista.html")]
pub struct CourseAnalistaTemplate {
    pub year: i32,
    pub handle: String,
    pub account_role: String,
    /// Precio de lanzamiento en USD (desde `domain::pricing`).
    pub price_usd: i32,
    /// Prefill del form si el visitante ya tiene sesión.
    pub prefill_name: String,
    pub prefill_email: String,
}

// ---------- settings / payment methods ----------

pub struct PaymentMethodView {
    pub id: String,
    pub rail_label: String,
    pub rail_slug: String,
    pub label: String,
    pub short_display: String,
    pub is_default: bool,
}

#[derive(Template)]
#[template(path = "settings/profile.html")]
pub struct ProfileTemplate {
    pub year: i32,
    pub handle: String,
    pub account_role: String,
    pub reports_total: i64,
    pub reports_valid: i64,
    pub reputation: i32,
    pub rank_label: String,
    pub has_payment_method: bool,
    pub completion_pct: i32,
}

#[derive(Template)]
#[template(path = "settings/payment_methods.html")]
pub struct PaymentMethodsTemplate {
    pub year: i32,
    pub handle: String,
    pub account_role: String,
    pub methods: Vec<PaymentMethodView>,
}

#[derive(Template)]
#[template(path = "settings/payment_methods_new.html")]
pub struct PaymentMethodsNewTemplate {
    pub year: i32,
    pub handle: String,
    pub account_role: String,
    pub rails: Vec<(String, String)>,
    pub initial_fields: String,
}

#[derive(Template)]
#[template(path = "settings/partials/payment_method_fields.html")]
pub struct PaymentMethodFieldsPartial {
    pub rail: String,
}

// ---------- companies ----------

pub struct CompanyMembershipView {
    pub slug: String,
    pub display_name: String,
    pub role: String,
    pub status: String,
}

#[derive(Template)]
#[template(path = "companies/index.html")]
pub struct CompaniesIndexTemplate {
    pub year: i32,
    pub handle: String,
    pub account_role: String,
    pub memberships: Vec<CompanyMembershipView>,
}

#[derive(Template)]
#[template(path = "companies/new.html")]
pub struct CompaniesNewTemplate {
    pub year: i32,
    pub handle: String,
    pub account_role: String,
}

pub struct ProgramRowView {
    pub slug: String,
    pub name: String,
    pub visibility: String,
    pub status: String,
    pub summary: String,
}

#[derive(Template)]
#[template(path = "companies/show.html")]
pub struct CompanyShowTemplate {
    pub year: i32,
    pub handle: String,
    pub account_role: String,
    pub company_slug: String,
    pub company_name: String,
    pub company_description: String,
    pub company_website: String,
    pub role: String,
    pub can_manage: bool,
    pub programs: Vec<ProgramRowView>,
}

// ---------- programs ----------

pub struct ProgramPublicCardView {
    pub company_slug: String,
    pub company_name: String,
    pub program_slug: String,
    pub name: String,
    pub summary: String,
    pub bounty_low: String,
    pub bounty_critical: String,
}

#[derive(Template)]
#[template(path = "programs/public_index.html")]
pub struct ProgramsPublicTemplate {
    pub year: i32,
    pub programs: Vec<ProgramPublicCardView>,
    pub handle: String,
    pub account_role: String,
}

/// Una fila de la tabla de precios base de referencia. Reutilizada por la home
/// (display) y el form de programa (defaults + hint). Derivada de
/// `domain::pricing`.
pub struct PriceTierView {
    pub emoji: String,
    pub label: String,
    /// Rango formateado, p.ej. "$100 – $300".
    pub range: String,
    /// Valor por defecto en USD (extremo inferior) para pre-rellenar el form.
    pub default_usd: i32,
    /// Clave estable (low/medium/high/critical).
    pub key: String,
}

#[derive(Template)]
#[template(path = "home.html")]
pub struct HomeTemplate {
    pub year: i32,
    pub programs: Vec<ProgramPublicCardView>,
    /// IDs de logo aliado (1..=17, sin 14 que no existe en redseg.org).
    /// Duplicados para que el marquee CSS haga loop continuo.
    pub ally_ids: Vec<i32>,
    pub handle: String,
    pub account_role: String,
}

#[derive(Template)]
#[template(path = "programs/new.html")]
pub struct ProgramNewTemplate {
    pub year: i32,
    pub handle: String,
    pub account_role: String,
    pub company_slug: String,
    pub company_name: String,
    /// Tramos de precio recomendados, para pre-rellenar y guiar los montos.
    pub tiers: Vec<PriceTierView>,
}

pub struct AssetRowView {
    pub id: String,
    pub asset_type: String,
    pub type_label: String,
    pub label: String,
    pub target: String,
    pub in_scope: bool,
    pub severity_cap: String,
}

#[derive(Template)]
#[template(path = "programs/show.html")]
pub struct ProgramShowTemplate {
    pub year: i32,
    pub public_view: bool,
    pub can_manage: bool,
    pub company_slug: String,
    pub company_name: String,
    pub program_slug: String,
    pub program_name: String,
    pub summary: String,
    pub policy_html: String,
    pub visibility: String,
    pub status: String,
    pub bounty_low: String,
    pub bounty_medium: String,
    pub bounty_high: String,
    pub bounty_critical: String,
    pub assets: Vec<AssetRowView>,
    pub handle: String,
    pub account_role: String,
    pub total_reports: i64,
    pub resolved_reports: i64,
    pub avg_response: String,
}

// ---------- assets ----------

#[derive(Template)]
#[template(path = "assets/new.html")]
pub struct AssetNewTemplate {
    pub year: i32,
    pub handle: String,
    pub account_role: String,
    pub company_slug: String,
    pub program_slug: String,
    pub program_name: String,
    pub types: Vec<(String, String)>,
    pub initial_fields: String,
}

#[derive(Template)]
#[template(path = "assets/partials/fields.html")]
pub struct AssetFieldsPartial {
    pub asset_type: String,
}

// ---------- reports ----------

#[derive(Template)]
#[template(path = "reports/new.html")]
pub struct ReportFormTemplate {
    pub year: i32,
    pub handle: String,
    pub account_role: String,
    pub company_slug: String,
    pub company_name: String,
    pub program_slug: String,
    pub program_name: String,
    pub assets: Vec<(String, String)>,
}

pub struct MyReportRow {
    pub public_id: String,
    pub title: String,
    pub state: String,
    pub severity: String,
}

#[derive(Template)]
#[template(path = "reports/list.html")]
pub struct ReportListTemplate {
    pub year: i32,
    pub handle: String,
    pub account_role: String,
    pub reports: Vec<MyReportRow>,
}

#[derive(Template)]
#[template(path = "reports/triage_list.html")]
pub struct TriageListTemplate {
    pub year: i32,
    pub handle: String,
    pub account_role: String,
    pub company_slug: String,
    pub program_slug: String,
    pub program_name: String,
    pub reports: Vec<MyReportRow>,
}

pub struct EventView {
    pub event_type: String,
    pub body_html: String,
    pub metadata_text: String,
    pub is_internal: bool,
    pub at: String,
}

pub struct AttachmentView {
    pub id: String,
    pub filename: String,
    pub mime: String,
    pub size_human: String,
    pub kind: String,
    pub sha256_short: String,
    pub can_delete: bool,
}

// ---------- payouts ----------

pub struct PayoutQueueRow {
    pub id: String,
    pub report_public_id: String,
    pub reporter_handle: String,
    pub rail: String,
    pub amount_usd: String,
    pub status: String,
    pub tx_ref: String,
    pub error_message: String,
}

#[derive(Template)]
#[template(path = "payouts/queue.html")]
pub struct PayoutsQueueTemplate {
    pub year: i32,
    pub handle: String,
    pub account_role: String,
    pub company_slug: String,
    pub company_name: String,
    pub escrow_usd: String,
    pub payouts: Vec<PayoutQueueRow>,
}

#[derive(Template)]
#[template(path = "payouts/mine.html")]
pub struct MinePayoutsTemplate {
    pub year: i32,
    pub handle: String,
    pub account_role: String,
    pub payouts: Vec<PayoutQueueRow>,
}

#[derive(serde::Deserialize, Debug)]
pub struct EscrowDepositForm {
    pub amount_usd: String,
}

// ---------- admin ----------

pub struct AdminAuditRow {
    pub id: i64,
    pub at: String,
    pub actor: String,
    pub actor_ip: String,
    pub action: String,
    pub target_type: String,
    pub target_id: String,
    pub metadata: String,
}

#[derive(Template)]
#[template(path = "admin/audit.html")]
pub struct AdminAuditTemplate {
    pub year: i32,
    pub handle: String,
    pub account_role: String,
    pub rows: Vec<AdminAuditRow>,
    pub action_prefix: String,
    pub target_type: String,
    pub target_id: String,
}

// ---------- osint ----------

/// Fila de un informe OSINT en listados (mine / review / catálogo).
pub struct OsintRowView {
    pub public_id: String,
    pub title: String,
    pub subject_name: String,
    pub category_label: String,
    pub criticality: String,
    pub status: String,
    pub status_label: String,
    /// Lo que gana el investigador ($50 base).
    pub price_usd: String,
    /// Lo que paga la empresa (vacío hasta que el admin lo fija).
    pub resale_usd: String,
    pub created: String,
}

#[derive(Template)]
#[template(path = "osint/new.html")]
pub struct OsintNewTemplate {
    pub year: i32,
    pub handle: String,
    pub account_role: String,
    pub categories: Vec<(String, String)>,
    pub severities: Vec<(String, String)>,
    pub osint_base_usd: i32,
}

#[derive(Template)]
#[template(path = "osint/mine.html")]
pub struct OsintMineTemplate {
    pub year: i32,
    pub handle: String,
    pub account_role: String,
    pub rows: Vec<OsintRowView>,
}

#[derive(Template)]
#[template(path = "osint/review.html")]
pub struct OsintReviewTemplate {
    pub year: i32,
    pub handle: String,
    pub account_role: String,
    pub rows: Vec<OsintRowView>,
}

#[derive(Template)]
#[template(path = "osint/catalog.html")]
pub struct OsintCatalogTemplate {
    pub year: i32,
    pub handle: String,
    pub account_role: String,
    pub company_slug: String,
    pub company_name: String,
    pub escrow_usd: String,
    pub rows: Vec<OsintRowView>,
}

#[derive(Template)]
#[template(path = "osint/show.html")]
pub struct OsintShowTemplate {
    pub year: i32,
    pub handle: String,
    pub account_role: String,
    pub public_id: String,
    pub title: String,
    pub subject_name: String,
    pub category_label: String,
    pub criticality: String,
    pub status: String,
    pub status_label: String,
    pub summary_html: String,
    pub body_html: String,
    pub can_see_body: bool,
    pub price_usd: String,
    pub resale_usd: String,
    /// Admin de la plataforma: puede aceptar/rechazar.
    pub can_review: bool,
    /// Miembro de la empresa-objetivo: puede comprar.
    pub can_buy: bool,
    pub created: String,
}

#[derive(Template)]
#[template(path = "reports/show.html")]
pub struct ReportShowTemplate {
    pub year: i32,
    pub handle: String,
    pub account_role: String,
    pub public_id: String,
    pub title: String,
    pub state: String,
    pub severity: String,
    pub description_html: String,
    pub impact_html: String,
    pub repro_html: String,
    pub cwe: String,
    pub cvss_vector: String,
    pub bounty_usd: String,
    pub is_triager: bool,
    pub is_reporter: bool,
    pub next_states: Vec<String>,
    pub events: Vec<EventView>,
    pub attachments: Vec<AttachmentView>,
}
