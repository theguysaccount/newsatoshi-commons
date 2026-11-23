# Stage 1: Build the Vite + React + Tailwind frontend bundle
FROM node:22-bookworm-slim AS web-builder
WORKDIR /build/web
COPY web/package.json web/package-lock.json ./
RUN npm ci
COPY web/ ./
RUN npm run build

# Stage 2: Build release Rust binaries (newsatoshi-commons + newsatoshi-hw-signer)
FROM rust:bookworm AS rust-builder
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY src/ ./src/
RUN cargo build --release --bins

# Stage 3: Minimal production runtime with CA certs for live HTTPS newsatoshi-seed polling
FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=rust-builder /build/target/release/newsatoshi-commons /usr/local/bin/newsatoshi-commons
COPY --from=rust-builder /build/target/release/newsatoshi-hw-signer /usr/local/bin/newsatoshi-hw-signer
COPY --from=rust-builder /build/target/release/newsatoshi-swap /usr/local/bin/newsatoshi-swap
COPY --from=web-builder /build/web/dist /app/web/dist
RUN mkdir -p /data
EXPOSE 8080
ENV COMMONS_PORT=8080
ENV COMMONS_STATE_FILE=/data/commons-ledger.json
ENV COMMONS_SEED_URL=https://newsatoshi-seed.fly.dev/status
ENV COMMONS_MAINNET_SEED_URL=https://newsatoshi.org/mainnet/status
ENV COMMONS_VERIFY_ONLY=1
ENTRYPOINT ["/bin/sh", "-c", "exec newsatoshi-commons serve --host 0.0.0.0 --port ${COMMONS_PORT} --state-file ${COMMONS_STATE_FILE} --seed-url ${COMMONS_SEED_URL} --mainnet-seed-url ${COMMONS_MAINNET_SEED_URL} --static-dir /app/web/dist"]
