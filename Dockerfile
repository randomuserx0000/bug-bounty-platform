# =============================================================================
# Dockerfile multi-stage para bugbounty-platform
#
# Estrategia: builder grande para compilar (rust + toolchain), runtime mínimo
# debian-slim con solo el binario + assets (templates/migrations/static).
#
# Optimización de caché: copiamos primero Cargo.{toml,lock} y compilamos un
# binario dummy para que la capa de dependencias se reuse mientras el
# código fuente cambie. Cargo-chef sería más fino pero esto alcanza.
# =============================================================================

# -----------------------------------------------------------------------------
# Stage 1: builder
# -----------------------------------------------------------------------------
FROM rust:1.83-slim-bookworm AS builder

# pkg-config y libssl no son necesarios — usamos rustls. ca-certificates por
# si crates.io hace fetches durante el build.
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates pkg-config \
    && rm -rf /var/lib/apt/lists/*

# Limitar paralelismo y desactivar LTO fat para evitar OOM en VPS pequeños.
# Sin estos overrides, el linker final pide 6+ GB de RAM por el LTO=fat del
# Cargo.toml. Con LTO=thin baja a ~2 GB; con jobs=2 el compilador paralelo
# usa la mitad de cores. Trade-off: ~10-15% más slow al runtime y ~20% más
# largo al build, pero estable en VPS de 4 GB con poco swap.
#
# Se setea vía env var (no editando Cargo.toml) para no invalidar la layer
# cache del builder de Docker — sino cada deploy recompilaría las deps de
# cero (~10 min extra).
ENV CARGO_BUILD_JOBS=2
ENV CARGO_PROFILE_RELEASE_LTO=thin

WORKDIR /app

# Caché de dependencias: copiamos solo manifests y compilamos con un main
# vacío para que las deps queden en el layer.
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
RUN mkdir -p src \
    && echo 'fn main() { println!("dummy"); }' > src/main.rs \
    && cargo build --release \
    && rm -rf src target/release/deps/bugbounty* target/release/bugbounty*

# Ahora copiamos el código real y compilamos en serio.
COPY src ./src
COPY migrations ./migrations
COPY templates ./templates
COPY static ./static

# Re-buildea solo el bin (las deps ya están cacheadas).
RUN cargo build --release --bin bugbounty

# -----------------------------------------------------------------------------
# Stage 2: runtime
# -----------------------------------------------------------------------------
FROM debian:bookworm-slim AS runtime

# ca-certificates para TLS saliente (Google OAuth, MinIO, etc.) y curl para el
# HEALTHCHECK (necesario para el deploy zero-downtime de Coolify).
# No hace falta libpq porque sqlx-postgres habla protocolo nativo con rustls.
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && useradd -u 1001 -U -m -s /sbin/nologin bb

WORKDIR /app

# Binario + assets necesarios en runtime.
COPY --from=builder --chown=bb:bb /app/target/release/bugbounty /usr/local/bin/bugbounty
COPY --chown=bb:bb migrations ./migrations
COPY --chown=bb:bb templates ./templates
COPY --chown=bb:bb static ./static

USER bb

ENV BIND_ADDR=0.0.0.0:8080 \
    RUST_LOG=info,bugbounty=info,sqlx=warn,tower_http=warn

EXPOSE 8080

# HEALTHCHECK del contenedor: Coolify lo usa para el deploy zero-downtime —
# mantiene el contenedor viejo sirviendo hasta que el nuevo reporte `healthy`,
# evitando el "no available server" durante el redeploy.
# - /readyz devuelve 200 solo si la BD está accesible (migraciones ya corrieron
#   antes de bindear el puerto en main.rs).
# - start-period amplio para dar tiempo a conectar BD + aplicar migraciones.
HEALTHCHECK --interval=15s --timeout=3s --start-period=40s --retries=3 \
    CMD curl -fsS http://127.0.0.1:8080/readyz || exit 1

CMD ["bugbounty"]
