# Sandbox Python ASAN Image — Python with AddressSanitizer + UndefinedBehaviorSanitizer
FROM python:3.11-slim

# Install build dependencies for ASAN-instrumented Python
RUN apt-get update && apt-get install -y --no-install-recommends \
    gcc g++ make libasan6 libubsan1 \
    && rm -rf /var/lib/apt/lists/*

# Set sanitizer environment variables
ENV ASAN_OPTIONS=detect_leaks=1:halt_on_error=1:abort_on_error=1:print_stacktrace=1
ENV UBSAN_OPTIONS=print_stacktrace=1:halt_on_error=1
ENV TSAN_OPTIONS=halt_on_error=1:history_size=7
ENV LSAN_OPTIONS=print_suppressions=0

# Preload sanitizer libraries for Python C extensions
ENV LD_PRELOAD=/usr/lib/x86_64-linux-gnu/libasan.so.6:/usr/lib/x86_64-linux-gnu/libubsan.so.1

# Install ctypes-compatible packages for sanitizer testing
RUN pip install --no-cache-dir cffi

WORKDIR /sandbox
RUN mkdir -p /tmp/sandbox

# Label
LABEL com.bugswarm.sanitizer="asan+ubsan"
LABEL com.bugswarm.phase="16"
