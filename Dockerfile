# syntax=docker/dockerfile:1

ARG RUST_VERSION=1.84
ARG APP_NAME=rsomhap

################################################################################
# Build stage

FROM rust:${RUST_VERSION} AS builder
ARG APP_NAME
WORKDIR /usr/src/app

COPY Cargo.toml Cargo.lock ./
COPY src ./src

RUN --mount=type=cache,target=/usr/src/app/target \
    --mount=type=cache,target=/usr/local/cargo/registry \
    cargo build --release --locked --bin ${APP_NAME} \
    && cp ./target/release/${APP_NAME} ./${APP_NAME}

################################################################################
# Runtime stage

FROM debian:bookworm-slim AS final
ARG APP_NAME

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /usr/src/app

ARG UID=10001
RUN adduser \
    --disabled-password \
    --gecos "" \
    --home "/nonexistent" \
    --shell "/sbin/nologin" \
    --no-create-home \
    --uid "${UID}" \
    appuser

# Copy the executable from the build stage.
COPY --from=builder --chown=appuser:appuser /usr/src/app/${APP_NAME} .

# Copy the necessary files.
COPY --chown=appuser:appuser templates ./templates
COPY --chown=appuser:appuser static ./static
COPY --chown=appuser:appuser config.toml ./

USER appuser

EXPOSE 5299

CMD ["./rsomhap"]