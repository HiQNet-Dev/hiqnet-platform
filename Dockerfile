FROM rust:1.99-bookworm AS builder
WORKDIR /app
COPY . .
RUN cargo build --release -p hiqnet-platform-api

FROM debian:bookworm-slim AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates wget \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --create-home --shell /bin/bash hiqnet
USER hiqnet
WORKDIR /home/hiqnet
COPY --from=builder /app/target/release/hiqnet-platform-api /usr/local/bin/hiqnet-platform-api
EXPOSE 8080
HEALTHCHECK --interval=10s --timeout=5s --retries=5 \
  CMD wget -qO- http://127.0.0.1:8080/health >/dev/null 2>&1 || exit 1
ENTRYPOINT ["hiqnet-platform-api"]
