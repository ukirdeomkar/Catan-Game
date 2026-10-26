# --- build stage ---
# rust:1.99-alpine (pinned by digest; bump via Dependabot docker PRs)
FROM rust:1.99-alpine@sha256:b1d5dd5a22a09404d3e798c0cf051b689077360374eb505fa5bb93c479693d87 AS build
RUN apk add --no-cache build-base
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY build.rs ./
COPY src ./src
COPY static ./static
RUN cargo build --release --locked

# --- runtime stage ---
# alpine:3.20 (pinned by digest; bump via Dependabot docker PRs)
FROM alpine:3.20@sha256:d9e853e87e55526f6b2917df91a2115c36dd7c696a35be12163d44e6e2a4b6bc
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
