# Changelog

All notable IronRoute changes are documented here.

## [Unreleased]

## [0.1.0] - 2026-09-05

### Added

- async Rust reverse proxy with bounded request bodies and upstream timeouts
- health-aware weighted upstream routing and active health checks
- per-upstream circuit breakers with half-open recovery
- bounded token-bucket client rate limiting and global load shedding
- retries restricted to semantically idempotent HTTP methods
- HMAC-SHA256 request verification with bounded timestamp skew
- hop-by-hop header sanitization and peer-derived forwarding identity
- health, readiness and Prometheus metrics endpoints
- typed TOML configuration validation, structured tracing and graceful shutdown
- socket-level end-to-end proxy and security regression tests
- Rust 1.97.1/1.98.1 CI, Clippy, RustSec audit and cargo-deny policy
- rootless production container build
- native release binaries, SHA-256 manifest and GHCR distribution automation
- OCI metadata, container SBOM and build provenance generation

### Security

- client-supplied `Forwarded`, `X-Forwarded-*` and `X-Real-IP` identity is stripped before proxying
- HMAC secrets are loaded from environment variables and never from checked-in configuration
- non-idempotent requests are not retried by the gateway
- dependency advisories and license/source policy are release-gating checks

[Unreleased]: https://github.com/mkarson1997/karzoun-ironroute/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/mkarson1997/karzoun-ironroute/releases/tag/v0.1.0
