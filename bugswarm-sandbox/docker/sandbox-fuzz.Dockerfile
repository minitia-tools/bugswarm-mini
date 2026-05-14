# Phase 20: AFL++ Fuzzing Sandbox Image
# Phase 21E: Danger-Guided Power Schedule Plugin

FROM ubuntu:22.04 AS builder

ENV DEBIAN_FRONTEND=noninteractive

# Install build dependencies + AFL++
RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential \
    git \
    make \
    cmake \
    gdb \
    valgrind \
    python3 \
    wget \
    ca-certificates \
    llvm-14 \
    llvm-14-dev \
    clang-14 \
    libc6-dev-i386 \
    && rm -rf /var/lib/apt/lists/*

# Build AFL++ from source
RUN git clone --depth=1 --branch v4.10c \
    https://github.com/AFLplusplus/AFLplusplus.git /opt/AFLplusplus \
    && cd /opt/AFLplusplus \
    && make -j$(nproc) distrib \
    && make install

# Build the danger power schedule plugin
COPY docker/danger_power_schedule.c /tmp/danger_power_schedule.c
RUN gcc -shared -fPIC -O2 -o /usr/local/lib/afl/danger_power_schedule.so \
    /tmp/danger_power_schedule.c \
    -lrt -lm \
    && strip /usr/local/lib/afl/danger_power_schedule.so \
    && rm /tmp/danger_power_schedule.c

# ─── Runtime Stage ─────────────────────────────────────────────────────────

FROM ubuntu:22.04

ENV DEBIAN_FRONTEND=noninteractive

# Runtime dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    libc6 \
    gdb \
    valgrind \
    python3 \
    && rm -rf /var/lib/apt/lists/*

# Copy AFL++ binaries from builder
COPY --from=builder /usr/local/bin/afl-* /usr/local/bin/
COPY --from=builder /usr/local/lib/afl/ /usr/local/lib/afl/

# Copy danger power schedule plugin
COPY --from=builder /usr/local/lib/afl/danger_power_schedule.so \
    /usr/local/lib/afl/danger_power_schedule.so

# Default environment for danger-guided fuzzing
ENV AFL_CUSTOM_MUTATOR_LIBRARY=/usr/local/lib/afl/danger_power_schedule.so
ENV BS_DANGER_WEIGHT=0.7
ENV BS_COVERAGE_WEIGHT=0.3
ENV BS_DANGER_SHM_NAME=/bugswarm_danger_map

# AFL standard environment
ENV AFL_SKIP_CPUFREQ=1
ENV AFL_I_DONT_CARE_ABOUT_MISSING_CRASHES=1
ENV AFL_NO_AFFINITY=1

# Working directories
RUN mkdir -p /corpus/in /corpus/out /corpus/out/crashes /corpus/out/queue

WORKDIR /sandbox

# Default entrypoint: run afl-fuzz with the danger map if available
ENTRYPOINT ["/usr/local/bin/afl-fuzz"]
