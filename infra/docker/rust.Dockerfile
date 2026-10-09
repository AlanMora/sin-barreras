# syntax=docker/dockerfile:1.7
# Imágenes de los servicios Rust del workspace. Una sola etapa compila todos
# los binarios (BuildKit la reutiliza entre imágenes) y cada imagen final toma
# el suyo:
#   docker build -f infra/docker/rust.Dockerfile --target bff .
#
# Etapa final: distroless/cc (glibc, sin shell ni gestor de paquetes),
# usuario no-root. El healthcheck lo hace el propio binario (`app healthcheck`).

ARG RUST_VERSION=1.99.0

FROM rust:${RUST_VERSION}-slim-trixie AS build
ARG RUST_VERSION
# Ignora rust-toolchain.toml dentro de la imagen: la toolchain ya viene
# instalada y no queremos descargas extra en el build.
ENV RUSTUP_TOOLCHAIN=${RUST_VERSION}
WORKDIR /src
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,target=/src/target,sharing=locked \
    cargo build --release --locked --bins \
    && mkdir -p /out \
    && for bin in bff record-writer c5-mock sb-admin; do install "target/release/$bin" "/out/$bin"; done

FROM gcr.io/distroless/cc-debian13:nonroot AS runtime-base
USER nonroot:nonroot
EXPOSE 8080
HEALTHCHECK --interval=10s --timeout=3s --start-period=5s --retries=3 CMD ["/app", "healthcheck"]
ENTRYPOINT ["/app"]

FROM runtime-base AS bff
COPY --from=build /out/bff /app

FROM runtime-base AS record-writer
COPY --from=build /out/record-writer /app

FROM runtime-base AS c5-mock
COPY --from=build /out/c5-mock /app

# Herramienta de administración (migraciones, verificación, cola de errores).
FROM gcr.io/distroless/cc-debian13:nonroot AS sb-admin
COPY --from=build /out/sb-admin /sb-admin
USER nonroot:nonroot
ENTRYPOINT ["/sb-admin"]
