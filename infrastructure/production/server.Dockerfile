# syntax=docker/dockerfile:1.7
# One image for the API (`serve`), the web viewer (`serve --mode web`), migrations and the OSM import.
FROM rust:1.99.1-trixie AS build
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY crates crates
COPY apps/server apps/server
# Cache mounts keep the registry and incremental target dir between builds (same effect as cargo-chef).
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/build/target \
    cargo build --release --locked -p safeeat-server \
 && cp target/release/safeeat-server /safeeat-server

FROM debian:trixie-slim
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates \
 && rm -rf /var/lib/apt/lists/* \
 && useradd --uid 1000 --user-group --no-create-home --shell /usr/sbin/nologin safeeat
ENV PORT=4000 MIGRATIONS_DIR=/app/migrations RUST_LOG=info
WORKDIR /app
COPY --from=build /safeeat-server /usr/local/bin/safeeat-server
COPY db/migrations ./migrations
USER safeeat
EXPOSE 4000
HEALTHCHECK --interval=20s --timeout=5s --start-period=10s CMD ["safeeat-server", "healthcheck", "http://127.0.0.1:4000/health"]
ENTRYPOINT ["safeeat-server"]
CMD ["serve"]
