FROM ubuntu:22.04@sha256:edf4aaae5d402c45bb2ae8afa8a9deb0cd2b194efdb858a67208424c0be2a4e4
LABEL version="1.0.0"
RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential pkg-config libz3-dev z3 \
    && rm -rf /var/lib/apt/lists/*
COPY target/release/bugswarm-symbolic /usr/local/bin/
CMD ["bugswarm-symbolic"]
