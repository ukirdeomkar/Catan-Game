# --- build stage ---
FROM rust:1.99-alpine AS build
RUN apk add --no-cache build-base
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY static ./static
RUN cargo build --release

# --- runtime stage ---
FROM alpine:3.20
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
