# syntax=docker/dockerfile:1

FROM node:22-alpine AS frontend
WORKDIR /web
COPY frontend/package.json frontend/package-lock.json ./
RUN npm ci
COPY frontend/ ./
RUN npm run build

FROM rust:1-bookworm AS backend
WORKDIR /src
COPY server/Cargo.toml server/Cargo.lock ./
COPY server/src ./src
RUN cargo build --release --locked

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=backend /src/target/release/cazt /usr/local/bin/cazt
COPY --from=frontend /web/dist /usr/share/cazt/web
ENV CAZT_WEB_ROOT=/usr/share/cazt/web \
    CAZT_BIND=0.0.0.0 \
    CAZT_PORT=8787 \
    CAZT_LOG=info
EXPOSE 8787
USER nobody
ENTRYPOINT ["/usr/local/bin/cazt"]
