FROM rust:1.98.0-bookworm AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release --locked

FROM debian:bookworm-slim
ARG VERSION=dev
LABEL org.opencontainers.image.title="Karzoun IronRoute" \
      org.opencontainers.image.description="Adaptive Rust edge gateway and resilience engine" \
      org.opencontainers.image.source="https://github.com/mkarson1997/karzoun-ironroute" \
      org.opencontainers.image.version="${VERSION}" \
      org.opencontainers.image.licenses="Apache-2.0"
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 65532 --no-create-home --shell /usr/sbin/nologin ironroute
COPY --from=build /src/target/release/karzoun-ironroute /usr/local/bin/ironroute
USER 65532:65532
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/ironroute"]
CMD ["serve", "--config", "/etc/ironroute/ironroute.toml"]
