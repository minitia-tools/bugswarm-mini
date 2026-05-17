FROM rust:1.80-bookworm AS builder
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY src/ src/
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /build/target/release/bugswarm-evidence /usr/local/bin/
ENTRYPOINT ["bugswarm-evidence"]
CMD ["run-server", "--socket", "/var/run/bugswarm/evidence.sock"]
HEALTHCHECK --interval=30s --timeout=5s --retries=3 CMD bugswarm-evidence health || exit 1
EXPOSE 8081
