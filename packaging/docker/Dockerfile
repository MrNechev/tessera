# syntax=docker/dockerfile:1
# Tessera as a self-hosted web app. Build from the repository root:
#   docker build -f packaging/docker/Dockerfile -t tessera .

# 1) the web UI
FROM node:24-trixie-slim AS web
WORKDIR /src
COPY package.json package-lock.json ./
RUN npm ci --no-audit --no-fund
COPY index.html vite.config.ts svelte.config.js tsconfig.json ./
COPY public ./public
COPY src ./src
RUN npm run build

# 2) the server binary (Rust); image codecs link to Debian's libheif/libdav1d
FROM rust:1-trixie AS build
RUN apt-get update \
 && apt-get install -y --no-install-recommends libheif-dev libdav1d-dev nasm cmake clang pkg-config \
 && rm -rf /var/lib/apt/lists/*
WORKDIR /src
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release --locked -p tessera-server \
 && cp target/release/tessera-server /tessera-server

# 3) runtime: ffmpeg (VAAPI), libvips (fast thumbnails), HEIC decoder plugin, Mesa VA drivers
FROM debian:trixie-slim
RUN apt-get update \
 && apt-get install -y --no-install-recommends \
      ffmpeg libvips-tools libheif1 libheif-plugin-libde265 libdav1d7 \
      mesa-va-drivers intel-media-va-driver vainfo ca-certificates tzdata \
 && rm -rf /var/lib/apt/lists/*
COPY --from=build /tessera-server /usr/local/bin/tessera-server
COPY --from=web /src/dist /usr/share/tessera/web
ENV TESSERA_DOCKER=1 \
    TESSERA_WEB_DIR=/usr/share/tessera/web \
    TESSERA_DATA_DIR=/data \
    TESSERA_CACHE_DIR=/cache \
    TESSERA_OUTPUTS_DIR=/outputs \
    TESSERA_INBOX_DIR=/inbox \
    TESSERA_LIBRARIES_ROOT=/libraries \
    TESSERA_HTTP_ADDR=:8470 \
    TESSERA_HTTPS_ADDR=:8471
RUN mkdir -p /data /cache /outputs /inbox /libraries && chown 1000:1000 /data /cache /outputs /inbox
USER 1000:1000
EXPOSE 8470 8471
VOLUME ["/data", "/cache"]
HEALTHCHECK --interval=30s --timeout=5s --start-period=30s CMD ["tessera-server", "health"]
ENTRYPOINT ["tessera-server"]
