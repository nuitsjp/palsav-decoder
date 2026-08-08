# Contributing

Thank you for contributing to `palsav-decoder`.

## Before opening a pull request

1. Open an issue for behavior or schema changes so compatibility can be discussed first.
2. Do not commit real save files, player identifiers, generated game assets, or proprietary game code.
3. Add a focused test before fixing a defect or adding behavior.
4. Keep stdout limited to the documented JSON or NDJSON contract and send diagnostics to stderr.
5. Preserve stable, path-free warning codes and error messages.

Run the following checks locally:

```text
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
cargo llvm-cov --workspace --all-targets --locked --fail-under-lines 85 --fail-under-functions 80 --fail-under-regions 80
cargo audit
```

By submitting a contribution, you agree that it is licensed under GPL-3.0-or-later and that you have
the right to provide it under that license.
