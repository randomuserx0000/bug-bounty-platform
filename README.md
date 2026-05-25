# bugbounty-platform

Plataforma regional de **bug bounty** para Venezuela y LATAM, vertical de la
red [REDSEG](https://www.redseg.org). Stack: **Rust + Axum + Postgres + MinIO**,
templates Askama con HTMX, sin frameworks de JS frontend.

## Qué incluye el MVP

- Auth con email/password (argon2id, sesiones firmadas, rate-limit) + **Google Sign-In** (OAuth 2.0 / PKCE).
- **Empresas** crean programas con scope polimórfico (13 tipos de asset: web, api, mobile, infra, firmware, hardware, radio, ...).
- **Researchers** reportan vulnerabilidades con markdown (EasyMDE) + attachments (PoCs, firmware, pcaps, etc.) hasta 50 MB vía MinIO.
- **State machine** de reports validada en dos capas (estructural + por rol).
- **Payouts** con escrow prefondeado: hook automático en `resolved`, débito en `pending`, marcado manual con `tx_ref`, reembolso si falla.
- **Audit log** append-only de cada acción sensible (auth, mutaciones, dinero).
- **Listado público** de programas en `/` sin necesidad de registro.

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
