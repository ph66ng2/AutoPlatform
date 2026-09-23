FROM rust:1.98.1-bookworm AS builder
WORKDIR /src
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY crates ./crates
COPY spikes ./spikes
COPY tests ./tests
RUN cargo build --locked --release --bin api --bin worker

FROM debian:bookworm-slim AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /src/target/release/api /usr/local/bin/api
COPY --from=builder /src/target/release/worker /usr/local/bin/worker
USER nobody
ENV APP_ENV=local \
    HTTP_BIND=0.0.0.0:8080 \
    RUST_LOG=info
EXPOSE 8080
STOPSIGNAL SIGTERM
CMD ["/usr/local/bin/api"]
