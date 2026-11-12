# Debian 12 / glibc 2.36 is the release build baseline. No GitHub pipeline.
FROM rust:1.93.1-slim-bookworm@sha256:5b9332190bb3b9ece73b810cd1f1e9f06343b294ce184bcb067f0747d7d333ea
RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential pkg-config libx11-dev libxi-dev libxkbcommon-dev libwayland-dev libgl1-mesa-dev \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /source
ENV CARGO_HOME=/cargo CARGO_TARGET_DIR=/build
CMD ["cargo", "build", "--release", "--locked"]
