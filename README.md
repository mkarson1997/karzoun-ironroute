# Karzoun IronRoute

[![CI](https://github.com/mkarson1997/karzoun-ironroute/actions/workflows/ci.yml/badge.svg)](https://github.com/mkarson1997/karzoun-ironroute/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/mkarson1997/karzoun-ironroute)](https://github.com/mkarson1997/karzoun-ironroute/releases)
[![Rust](https://img.shields.io/badge/Rust-1.98.1-orange)](rust-toolchain.toml)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue)](LICENSE)

> A security-conscious adaptive edge gateway in Rust for health-aware routing, circuit breaking, bounded rate limiting, load shedding and signed internal traffic.

IronRoute is project **04/41** in the Karzoun engineering portfolio. It demonstrates networking, concurrency and resilience engineering in Rust through a working gateway rather than a synthetic algorithm demo.

## v0.1 capabilities

- async HTTP reverse proxy built on Tokio/Axum/Reqwest
- health-aware weighted upstream selection
- per-upstream circuit breakers with half-open recovery
- active health checks
- token-bucket rate limiting keyed from the TCP peer address
- bounded rate-limiter identity cardinality
- global in-flight concurrency limit and immediate load shedding
- retries only for semantically idempotent methods
- HMAC-SHA256 verification for configured protected path prefixes
- replay-window reduction with timestamp skew validation
- request body size limits and upstream timeouts
- hop-by-hop header and `Connection` token sanitization
- replacement of client-supplied forwarding identity with TCP peer-derived identity
- `/healthz`, `/readyz` and Prometheus `/metrics`
- structured JSON tracing and graceful shutdown
- socket-level end-to-end proxy tests
- RustSec advisory scanning and cargo-deny dependency policy

## Quick start

```bash
cp ironroute.example.toml ironroute.toml
cargo run -- serve --config ironroute.toml
```

Validate configuration without binding a socket:

```bash
cargo run -- check --config ironroute.toml
```

## Distribution

Tagged releases publish native binaries built on GitHub-hosted Linux, Windows and macOS runners. Each release includes a `SHA256SUMS.txt` manifest.

```bash
sha256sum -c SHA256SUMS.txt
```

The production container is published to GitHub Container Registry:

```bash
docker pull ghcr.io/mkarson1997/karzoun-ironroute:0.1.0

docker run --rm \
  -p 8080:8080 \
  -v "$PWD/ironroute.toml:/etc/ironroute/ironroute.toml:ro" \
  ghcr.io/mkarson1997/karzoun-ironroute:0.1.0
```

Release container builds include OCI source/version/license labels plus SBOM and build provenance metadata from BuildKit/GitHub Actions.

## HMAC protected routes

Enable `[hmac]` in the configuration and provide the secret out-of-band:

```bash
export IRONROUTE_HMAC_SECRET='replace-with-at-least-32-random-bytes'
```

The signature canonical form is:

```text
METHOD\nPATH_AND_QUERY\nUNIX_TIMESTAMP\nSHA256_HEX(BODY)
```

The HMAC is SHA-256 and the transmitted signature is lowercase hexadecimal. Timestamp skew is bounded. This reduces replay exposure but is not a nonce database; strict single-use semantics belong in a durable replay store.

## Forwarded identity

IronRoute does not trust client-provided `Forwarded`, `X-Forwarded-*` or `X-Real-IP` identity. Those headers are removed on the upstream request path and replaced with values derived from the actual TCP peer. Trusted-proxy chaining is intentionally deferred until a CIDR-aware trust policy exists.

## Retry safety

IronRoute retries GET, HEAD, OPTIONS, PUT and DELETE only. POST and PATCH are never retried by the gateway in v0.1. Applications should still use idempotency keys for externally visible side effects.

## Operational endpoints

```text
GET /healthz
GET /readyz
GET /metrics
```

`/readyz` fails when every upstream is unhealthy or unavailable behind an open circuit.

## Configuration

See [`ironroute.example.toml`](ironroute.example.toml). Configuration is typed and validated before the listener starts. Secrets are environment-backed, not stored in TOML.

## Engineering checks

Every pull request is gated by:

```text
Rust 1.97.1 + 1.98.1
cargo fmt
Clippy -D warnings
unit + socket-level integration tests
locked release build
RustSec cargo-audit
cargo-deny licenses/advisories/bans/sources
```

## Explicit boundaries

v0.1 does **not** claim to be a service mesh, Envoy replacement or complete zero-trust gateway. It does not yet terminate downstream TLS, support mTLS, hot reload configuration, or provide multi-process control-plane state. Those capabilities are roadmap milestones.

## Security

See [`SECURITY.md`](SECURITY.md). Never put credentials, production traffic captures or private infrastructure data in public issues.

## Contributing

See [`CONTRIBUTING.md`](CONTRIBUTING.md). Runtime and security-sensitive changes should include a regression test for the invariant they modify.

## License

Apache-2.0. See [`LICENSE`](LICENSE) and [`NOTICE`](NOTICE).
