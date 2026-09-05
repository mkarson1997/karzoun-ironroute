# Security Policy

IronRoute is an edge component and should be treated as security-sensitive infrastructure.

## Reporting

Please use GitHub private vulnerability reporting when available. Do not disclose exploitable details, credentials, internal hostnames, production traffic samples, HMAC secrets, private keys or tokens in public issues.

## v0.1 security boundaries

- HMAC secrets are loaded from an environment variable and are never read from the TOML file.
- Signed requests bind method, path/query, timestamp and body digest.
- Clock-skew validation limits replay windows but is not a nonce store; deployments requiring strict single-use requests must add replay-state at the application boundary.
- `X-Forwarded-For` from clients is not trusted in v0.1; IronRoute derives client identity from the TCP peer.
- Retries are disabled for POST/PATCH and other non-idempotent methods.
- Downstream TLS termination and mTLS are roadmap items, not v0.1 claims.
