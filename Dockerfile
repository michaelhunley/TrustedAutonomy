# Trusted Autonomy — OCI container image
#
# Multi-stage build: compile from source, produce minimal runtime image.
#
# Usage:
#   docker build -t ta .
#   docker run -it -v $(pwd):/workspace ta serve
#   docker run -it -v $(pwd):/workspace ta dev
#
# For daemon mode (web UI + MCP server):
#   docker run -d -p 3000:3000 -v $(pwd):/workspace ta daemon
#
# Environment variables:
#   TA_PROJECT_ROOT — override project root (default: /workspace)
#   TA_LOG_LEVEL    — tracing log level (default: info)

# ── Stage 1: Build ──────────────────────────────────────────────
FROM rust:1.82-bookworm AS builder

WORKDIR /src

# Copy manifests first for dependency caching.
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY crates/ crates/
COPY apps/ apps/

# Build the ta-cli binary in release mode.
RUN cargo build --release -p ta-cli \
    && strip target/release/ta

# ── Stage 2: Runtime ────────────────────────────────────────────
FROM debian:bookworm-slim AS runtime

RUN apt-get update && apt-get install -y --no-install-recommends \
        git \
        ca-certificates \
        jq \
    && rm -rf /var/lib/apt/lists/*

# Copy the built binary.
COPY --from=builder /src/target/release/ta /usr/local/bin/ta

# Copy agent configs and templates.
COPY agents/ /usr/local/share/ta/agents/
COPY docs/USAGE.md /usr/local/share/ta/USAGE.md

# Default workspace mount point.
VOLUME /workspace
WORKDIR /workspace

ENV TA_PROJECT_ROOT=/workspace
ENV TA_LOG_LEVEL=info

# Expose web UI port (for daemon mode).
EXPOSE 3000

ENTRYPOINT ["ta"]
CMD ["--help"]
