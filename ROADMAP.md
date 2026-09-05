# Roadmap

## v0.1 - Adaptive edge core

- reverse proxy
- health-aware weighted routing
- circuit breakers
- bounded token-bucket rate limiting
- load shedding
- idempotent-only retries
- HMAC request verification
- active health checks
- operational metrics and readiness

## v0.2 - Streaming and policy depth

- bounded streaming request path without buffering entire bodies
- route-specific policies and upstream pools
- hot configuration reload with atomic snapshots
- per-route retry budgets and concurrency limits
- richer Prometheus labels with cardinality controls

## v0.3 - Transport security

- downstream TLS with rustls
- upstream client certificates and mTLS
- certificate reload/rotation
- strict trusted-proxy CIDR handling

## v0.4 - Reliability engineering

- property tests for policy invariants
- fuzzing for configuration/header/signature parsers
- criterion benchmarks and reproducible load profiles
- graceful connection draining
- OpenTelemetry trace propagation
