FROM rust:1.86-bookworm AS builder
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY data ./data
RUN cargo build --release

FROM debian:bookworm-slim
RUN useradd --system --home /var/lib/typojet --shell /usr/sbin/nologin typojet \
    && mkdir -p /var/lib/typojet \
    && chown -R typojet:typojet /var/lib/typojet
WORKDIR /var/lib/typojet
COPY --from=builder /app/target/release/typojet /usr/local/bin/typojet
USER typojet
EXPOSE 7700
CMD ["typojet", "--bind", "0.0.0.0:7700", "--data-dir", "/var/lib/typojet"]
