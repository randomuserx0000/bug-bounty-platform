# Deploy a Coolify

Stack: **app (Rust+Axum)** + **Postgres 16** + **MinIO**, todo en un solo
`docker-compose.prod.yml`.

## Pre-requisitos

- Servidor con Coolify instalado.
- Dominio apuntando al servidor (A record). Ej: `bugbounty.ve` → IP del VPS.

## Paso 1 — Generar secretos

En tu máquina local:

```bash
# Key de 32 bytes para cifrar payment_methods.details_enc
openssl rand -hex 32

# Key de 64 bytes para firmar cookies de sesión
openssl rand -hex 64

# Password fuerte de Postgres y MinIO (cualquiera de los dos)
openssl rand -base64 24
```

Guardá los tres valores — los vas a pegar en Coolify.

## Paso 2 — En Coolify

1. **New Resource** → **Docker Compose**.
2. **Source**: apuntá al repo Git de este proyecto (o subí el código).
3. **Docker Compose Location**: `docker-compose.prod.yml`.
4. **Domains**: el dominio que apunta al servidor. Coolify provisiona HTTPS
   automático con Let's Encrypt.
5. **Environment Variables** (panel de Coolify):

   | Variable | Valor | Notas |
   |---|---|---|
   | `PUBLIC_URL` | `https://bugbounty.ve` | tu dominio real con `https://` y sin trailing slash |
   | `POSTGRES_PASSWORD` | el output de openssl | persiste — no lo cambies después de deploy |
   | `PAYMENT_METHODS_KEY_HEX` | 32 bytes hex (64 chars) | rotar este key requiere re-cifrar `payment_methods.details_enc` |
   | `COOKIE_KEY_HEX` | 64 bytes hex (128 chars) | rotar invalida todas las sesiones existentes |
   | `MINIO_ROOT_USER` | `minioadmin` o el que quieras | usuario del bucket |
   | `MINIO_ROOT_PASSWORD` | password fuerte | persiste — protege el bucket |
   | `GOOGLE_CLIENT_ID` | (opcional) | si tenés creds de Google Cloud para Sign-In |
   | `GOOGLE_CLIENT_SECRET` | (opcional) | idem |
   | `RUST_LOG` | (opcional) | default `info,bugbounty=info,sqlx=warn` |

6. **Deploy**. Coolify hace `docker compose build && up -d`. El primer build
   tarda 5–10 min (compilación Rust + sus dependencias).

## Paso 3 — Verificar

Después del deploy:

```bash
# Healthcheck público (no requiere auth)
curl https://bugbounty.ve/healthz   # → "ok"
curl https://bugbounty.ve/readyz    # → "ready" si Postgres está conectado
```

Abrí `https://bugbounty.ve/` en el browser — debería verse la landing pública.

## Paso 4 — Bootstrap del admin

Para acceder a `/admin/audit`, tu user necesita `role='admin'`. Después de
registrarte normalmente, abre la consola de Postgres en Coolify (servicio
`db`, comando `psql`) y corre:

```sql
UPDATE users SET role='admin' WHERE handle='tu_handle';
```

## Paso 5 (opcional) — Google Sign-In

1. Andá a https://console.cloud.google.com/apis/credentials
2. Crea **OAuth client ID** tipo **Web application**.
3. **Authorized redirect URIs**: `https://bugbounty.ve/auth/google/callback`
4. Pegá `Client ID` y `Client Secret` en las env vars de Coolify.
5. Redeploy. El botón "Continuar con Google" aparecerá automático en login/signup.

## Notas operativas

### Persistencia
Los volúmenes `pgdata` y `miniodata` viven en el host de Coolify. **Redeploy
no los borra**, pero si destruís el resource desde Coolify sí. Antes de borrar
un proyecto, hacé backup:

```bash
# En el servidor (vía SSH o consola web)
docker exec -t bugbounty-platform-db-1 pg_dump -U bb bugbounty > backup.sql
docker run --rm -v bugbounty-platform_miniodata:/data -v $PWD:/backup alpine tar czf /backup/minio.tgz /data
```

### Updates de código
`git push` al branch que Coolify mira → auto-deploy. Las migraciones
(`migrations/*.sql`) se aplican automáticamente al arranque vía
`sqlx::migrate!`. Si una migración falla, el container no arranca — chequeá
los logs.

### Logs
Coolify muestra el output de `tracing` en su consola. Para tail manual:

```bash
docker compose -f docker-compose.prod.yml logs -f app
```

### Email transaccional (pendiente)
Hoy el sender es `LogOnly`: los emails aparecen en los logs pero no se
envían. Para activarlos: integrar Resend/Postmark/SES (ver
`memory/project_platform_next_steps.md` Opción A).

### MinIO console (opcional)
Si querés ver el bucket vía web: en Coolify, expone el puerto `9001` del
servicio `minio` como un segundo dominio (ej: `minio.bugbounty.ve`). Login
con `MINIO_ROOT_USER` / `MINIO_ROOT_PASSWORD`.

### Object storage externo (Hetzner / S3)
Si los attachments crecen y querés salir de MinIO local:
1. Crear bucket en Hetzner Object Storage o AWS S3.
2. Cambiar en Coolify:
   - `S3_ENDPOINT=https://fsn1.your-objectstorage.com` (Hetzner) o quitar
     para S3 nativo.
   - `S3_ACCESS_KEY` y `S3_SECRET_KEY` del nuevo proveedor.
   - `S3_FORCE_PATH_STYLE=false` para S3 nativo.
3. Migrar los objetos existentes con `mc mirror local/bb-attachments s3/bb-attachments`.
4. Quitar el servicio `minio` y `minio-init` del compose.

## Troubleshooting

**El primer deploy se cuelga compilando**: normal, Rust con todas las deps
(aws-sdk-s3, oauth2, sqlx, etc.) tarda. Una vez compilado, builds incrementales
son rápidos gracias al cache del Dockerfile.

**`bucket 'bb-attachments' no accesible`**: el `minio-init` no terminó antes
que el `app` arrancara. El `depends_on.minio-init.condition` lo previene en
teoría; si pasa, redeploy.

**`401` o redirect loop al login**: revisar que `PUBLIC_URL` empiece con
`https://` (para que las cookies sean `Secure`). Sin HTTPS en prod, las
cookies no se setean.

**Google OAuth: `redirect_uri_mismatch`**: el URI que registraste en Google
Cloud Console debe ser EXACTAMENTE `https://tudominio/auth/google/callback`,
con el mismo dominio y protocolo que `PUBLIC_URL`.
