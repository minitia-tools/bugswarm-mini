FROM rust:1.80-bookworm AS builder
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY src/ src/
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /build/target/release/bugswarm-cpg /usr/local/bin/
ENTRYPOINT ["bugswarm-cpg"]
CMD ["daemon", "--socket", "/var/run/bugswarm/cpg.sock"]
HEALTHCHECK --interval=30s --timeout=5s --retries=3 CMD bugswarm-cpg health || exit 1
EXPOSE 8080
