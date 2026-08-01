# syntax=docker/dockerfile:1
#
# One image per service — select the binary at build time:
#   docker build --build-arg BIN=amaker-author  -t amaker-author  .
#   docker build --build-arg BIN=amaker-assess  -t amaker-assess  .
#   docker build --build-arg BIN=amaker-analyze -t amaker-analyze .
#
# The builder compiles the whole workspace once; with BuildKit cache
# mounts the 2nd and 3rd image builds reuse that compile.

# ---- builder: compile the three binaries + the Tailwind CSS bundle ----
# Pinned to bookworm so the produced binaries' glibc matches the runtime.
FROM rust:1-bookworm AS builder
WORKDIR /build

# Tailwind standalone CLI — builds assets/app.css from the templates.
ARG TAILWIND_VERSION=4.3.0
RUN curl -fsSL -o /usr/local/bin/tailwindcss \
        "https://github.com/tailwindlabs/tailwindcss/releases/download/v${TAILWIND_VERSION}/tailwindcss-linux-x64" \
    && chmod +x /usr/local/bin/tailwindcss

COPY . .
RUN tailwindcss -i assets/app.src.css -o assets/app.css --minify
RUN --mount=type=cache,target=/build/target \
    --mount=type=cache,target=/usr/local/cargo/registry \
    cargo build --release --workspace \
    && mkdir -p /out \
    && cp target/release/amaker-author \
          target/release/amaker-assess \
          target/release/amaker-analyze /out/

# ---- runtime: slim image, ENTRYPOINT picked by BIN ----
FROM debian:bookworm-slim AS runtime
ARG BIN
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
RUN useradd --system --uid 10001 amaker
WORKDIR /app
COPY --from=builder /out/${BIN} /app/server
COPY --from=builder /build/assets /app/assets
USER amaker

# Bind all interfaces inside the container — the platform routes its
# injected $PORT here. Each binary reads only its own <PREFIX>_HOST,
# so setting all three is harmless.
ENV AUTHOR_HOST=0.0.0.0 \
    ASSESS_HOST=0.0.0.0 \
    ANALYZE_HOST=0.0.0.0
EXPOSE 8080
ENTRYPOINT ["/app/server"]
