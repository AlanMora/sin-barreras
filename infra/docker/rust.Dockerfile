# syntax=docker/dockerfile:1.7
# Imagen de cualquiera de los servicios Rust del workspace.
#   docker build -f infra/docker/rust.Dockerfile --build-arg APP=bff .
#
# Etapa final: distroless/cc (glibc, sin shell ni gestor de paquetes),
# usuario no-root. El healthcheck lo hace el propio binario (`app healthcheck`).

ARG RUST_VERSION=1.99.0

FROM rust:${RUST_VERSION}-slim-trixie AS build
ARG APP
ARG RUST_VERSION
# Ignora rust-toolchain.toml dentro de la imagen: la toolchain ya viene
# instalada y no queremos descargas extra en el build.
ENV RUSTUP_TOOLCHAIN=${RUST_VERSION}
WORKDIR /src
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target,id=target-${APP} \
    test -n "${APP}" \
    && cargo build --release --locked -p "${APP}" \
    && install -D "target/release/${APP}" /out/app

FROM gcr.io/distroless/cc-debian13:nonroot
COPY --from=build /out/app /app
USER nonroot:nonroot
EXPOSE 8080
HEALTHCHECK --interval=10s --timeout=3s --start-period=5s --retries=3 CMD ["/app", "healthcheck"]
ENTRYPOINT ["/app"]
