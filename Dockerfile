# --- build stage ---
# rust:1.99-alpine (pinned by digest; bump via Dependabot docker PRs)
FROM rust:1.99-alpine@sha256:7e9e2d068a107a2adafd19930011178d14a78326053cd4c3a9f7ad3fd049557d AS build
RUN apk add --no-cache build-base
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY build.rs ./
COPY src ./src
COPY static ./static
RUN cargo build --release --locked

# --- runtime stage ---
# alpine:3.20 (pinned by digest; bump via Dependabot docker PRs)
FROM alpine:3.24@sha256:be2656061b536c1883c4034790e21d0a09c5aaaf51968b89368a3c0371ea2c9a
RUN adduser -D -u 10001 catan
WORKDIR /app
COPY --from=build /app/target/release/catan /app/catan
COPY static /app/static
RUN mkdir -p /data && chown catan:catan /data
USER catan
ENV CATAN_ADDR=0.0.0.0:8080
ENV CATAN_DATA_DIR=/data
VOLUME ["/data"]
EXPOSE 8080
CMD ["/app/catan"]
