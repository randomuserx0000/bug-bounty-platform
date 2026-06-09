# Escudo Digital

**Escudo Digital** — plataforma regional de **bug bounty** e **inteligencia OSINT**
para Venezuela y LATAM, vertical de la red [REDSEG](https://www.redseg.org).
No solo recibe reportes de vulnerabilidades: opera además un marketplace donde
investigadores venden informes OSINT sobre empresas y la plataforma los revende a
las empresas afiliadas para remediarlos. Stack: **Rust + Axum + Postgres + MinIO**,
templates Askama con HTMX, sin frameworks de JS frontend.

## Qué incluye el MVP

- Auth con email/password (argon2id, sesiones firmadas, rate-limit) + **Google Sign-In** (OAuth 2.0 / PKCE).
- **Empresas** crean programas con scope polimórfico (13 tipos de asset: web, api, mobile, infra, firmware, hardware, radio, ...).
- **Researchers** reportan vulnerabilidades con markdown (EasyMDE) + attachments (PoCs, firmware, pcaps, etc.) hasta 50 MB vía MinIO.
- **State machine** de reports validada en dos capas (estructural + por rol).
- **Payouts** con escrow prefondeado: hook automático en `resolved`, débito en `pending`, marcado manual con `tx_ref`, reembolso si falla.
- **Audit log** append-only de cada acción sensible (auth, mutaciones, dinero).
- **Listado público** de programas en `/` sin necesidad de registro.
- **Producto OSINT** (entidad separada): investigadores venden informes OSINT
  sobre empresas (precio base **$50**); un admin los revisa y fija el precio de
  reventa; la empresa-objetivo los compra desde su catálogo (`/manage/:slug/osint`),
  debitando su escrow. El margen (reventa − base) es ingreso de la plataforma.
- **Precios base de referencia** (`src/domain/pricing.rs`): OSINT $50 ·
  low $100–300 · medium $400–600 · high $700–900 · critical $1.000–2.000.
  Se muestran en la home y pre-rellenan el form de programa.

### Producto OSINT — estado y límites (v1)

- Flujo completo: enviar (`/osint/new`) → revisar (`/osint/review`, admin) →
  aceptar/rechazar → comprar (empresa). Gating del cuerpo: solo autor, admin o
  la empresa que compró ven el informe completo.
- **Límite v1**: el pago al investigador se registra en `osint_reports`
  (`price_cents`) y se notifica por email; el desembolso real se gestiona
  operativamente. La tabla `payouts` ya está preparada (columna `osint_report_id`)
  para integrarlo en una 2ª iteración.
- **Formación (OSINT Academy)**: diseñada en [docs/osint-academy.md](docs/osint-academy.md),
  aún no implementada.

## UI / UX (estado actual)

### Dashboard del researcher
Rediseño al estilo HackerOne/Intigriti/Bugcrowd: KPIs reales (reports totales, válidos, bounties 90d, reputación, ranking), action items, activity feed, payouts recientes y programas destacados.

### Perfil (`/settings/profile`)
- **Stats** en header: reports, válidos, reputación, ranking (datos reales de BD).
- **Completion bar**: progreso de 4 checkpoints (cuenta creada, nick, método de pago, primer reporte).
- **Achievements**: Bronze/Silver/Gold/Platinum Finder con progress bars reales según `reports_valid`.
- **Theme selector**: Light / Dark / Sistema — persistido en `localStorage`, sin columna en BD.

### Dark mode
Activado con `data-theme="dark"` en `<html>`. Script inline en `<head>` aplica el tema antes del primer paint para evitar flash. `bbSetTheme(t)` disponible globalmente para cualquier toggle. El CSS en `app.css` cubre topbar, cards, forms, hero, footer, EasyMDE y todos los componentes relevantes.

### Program cards
Cards estilo Intigriti: logo placeholder con inicial de empresa, badge público/privado, stats grid (avg paid, primera respuesta, aceptados, researchers — placeholders por ahora), bounty range al pie.

### Aliados
Marquee en home con logos de la red REDSEG + **SentinelOne** (autorización comercial). Logo SVG en `static/s1.svg`.

## Desarrollo local

Requiere Rust stable + podman (o docker) + docker-compose.

```bash
# 1. Postgres + MinIO + bucket
docker-compose up -d

# 2. .env (copiá .env.example y generá las keys)
cp .env.example .env
echo "PAYMENT_METHODS_KEY_HEX=$(openssl rand -hex 32)" >> .env
echo "COOKIE_KEY_HEX=$(openssl rand -hex 64)" >> .env

# 3. Arrancar
cargo run

# 4. Abrí http://localhost:8080
```

## Deploy a producción

Ver [DEPLOY.md](DEPLOY.md). Resumen: `docker-compose.prod.yml` + Coolify (o
cualquier orquestador docker-compose). Coolify maneja reverse proxy + SSL
automático.

## Roadmap

Pasos completados (8): auth, payments, programs+assets, reports, attachments,
payouts, audit. Los próximos abiertos (email transaccional real, TRC20
automation, mejoras UX) están listados en commits y memos internos.

## Licencia

Por definir.
