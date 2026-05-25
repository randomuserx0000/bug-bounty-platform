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

# Solo necesitamos ca-certificates para TLS saliente (Google OAuth, MinIO, etc.).
# No hace falta libpq porque sqlx-postgres habla protocolo nativo con rustls.
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
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

# Sin HEALTHCHECK nativo (debian-slim no trae wget/curl). Coolify hace
# probes HTTP a /healthz y /readyz por sí mismo desde su UI.

CMD ["bugbounty"]
