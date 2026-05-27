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
}

#[derive(Template)]
#[template(path = "dashboard.html")]
pub struct DashboardTemplate {
    pub year: i32,
    pub handle: String,
    pub role: String,
}

#[derive(Template)]
#[template(path = "signup.html")]
pub struct SignupTemplate {
    pub year: i32,
    pub google_enabled: bool,
    pub handle: String,
}

/// Fragmento de error de formulario. Lo inyecta HTMX en `#form-feedback`.
#[derive(Template)]
#[template(path = "partials/form_error.html")]
pub struct FormErrorPartial {
    pub message: String,
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
}

#[derive(Template)]
#[template(path = "settings/payment_methods.html")]
pub struct PaymentMethodsTemplate {
    pub year: i32,
    pub handle: String,
    pub methods: Vec<PaymentMethodView>,
}

#[derive(Template)]
#[template(path = "settings/payment_methods_new.html")]
pub struct PaymentMethodsNewTemplate {
    pub year: i32,
    pub handle: String,
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
    pub memberships: Vec<CompanyMembershipView>,
}

#[derive(Template)]
#[template(path = "companies/new.html")]
pub struct CompaniesNewTemplate {
    pub year: i32,
    pub handle: String,
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
}

#[derive(Template)]
#[template(path = "programs/new.html")]
pub struct ProgramNewTemplate {
    pub year: i32,
    pub handle: String,
    pub company_slug: String,
    pub company_name: String,
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
}

// ---------- assets ----------

#[derive(Template)]
#[template(path = "assets/new.html")]
pub struct AssetNewTemplate {
    pub year: i32,
    pub handle: String,
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
    pub reports: Vec<MyReportRow>,
}

#[derive(Template)]
#[template(path = "reports/triage_list.html")]
pub struct TriageListTemplate {
    pub year: i32,
    pub handle: String,
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
    pub rows: Vec<AdminAuditRow>,
    pub action_prefix: String,
    pub target_type: String,
    pub target_id: String,
}

#[derive(Template)]
#[template(path = "reports/show.html")]
pub struct ReportShowTemplate {
    pub year: i32,
    pub handle: String,
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
