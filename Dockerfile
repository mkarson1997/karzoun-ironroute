FROM rust:1.98.1-bookworm AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release --locked

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 65532 --no-create-home --shell /usr/sbin/nologin ironroute
COPY --from=build /src/target/release/karzoun-ironroute /usr/local/bin/ironroute
USER 65532:65532
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/ironroute"]
CMD ["serve", "--config", "/etc/ironroute/ironroute.toml"]
