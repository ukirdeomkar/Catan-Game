# --- build stage ---
# rust:1.99-alpine (pinned by digest; bump via Dependabot docker PRs)
FROM rust:1.100-alpine@sha256:8d0e82abfdf9a38071e07cabc5afba4ddbe077631a80e4e742a760c38beadc33 AS build
RUN apk add --no-cache build-base
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY build.rs ./
COPY src ./src
COPY static ./static
RUN cargo build --release --locked

# --- runtime stage ---
# alpine:3.20 (pinned by digest; bump via Dependabot docker PRs)
FROM alpine:3.24@sha256:f59abe656b692d901e184be3445e084786602108599d2a51d229c0f5a48c71bf
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
