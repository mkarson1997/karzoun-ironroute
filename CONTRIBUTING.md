# Contributing

Use a focused branch and pull request. Runtime changes should include tests for the failure mode or invariant they modify.

Before opening a PR:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo test --all-features --locked
cargo audit
cargo deny check
```

Never include production secrets, private traffic captures or internal infrastructure details in issues, tests or logs.
