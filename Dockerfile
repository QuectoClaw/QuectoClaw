# QuectoClaw — Multi-stage Docker build
# Produces a minimal image for gateway deployment.

# --- Stage 1: Build ---
FROM rust:1.83-slim-bookworm AS builder

RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build
COPY Cargo.toml Cargo.lock* ./
COPY src/ src/
COPY tests/ tests/

RUN cargo build --release \
    && strip target/release/quectoclaw

# --- Stage 2: Runtime ---
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

RUN groupadd -r quecto && useradd -r -g quecto -m quecto

COPY --from=builder /build/target/release/quectoclaw /usr/local/bin/quectoclaw

# Default workspace directory (mount a volume here)
RUN mkdir -p /home/quecto/.quectoclaw/workspace && chown -R quecto:quecto /home/quecto
VOLUME /home/quecto/.quectoclaw

USER quecto
WORKDIR /home/quecto

# Web dashboard port
EXPOSE 3000

ENTRYPOINT ["quectoclaw"]
CMD ["gateway"]
