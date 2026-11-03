# Contributing

## Before changing code

- Read `README.md`, `docs/LICENSING.md`, and the per-domain documentation.
- Do not add credentials, Cookie exports, tokens, personal paths, real account
  output, or private endpoints to examples, fixtures, logs, or commits.
- Check that any copied or translated code is compatible with the upstream
  licenses recorded under `licenses/`.

## Validation

Run all repository checks before opening a pull request:

```sh
cargo fmt --all -- --check
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
```

If live behavior is tested, use read-only operations first. Do not commit real
responses or account identifiers. Describe unavailable infrastructure or
verification boundaries explicitly.

Pull request CI is not triggered automatically. After reviewing a pull request,
the repository owner can start the `CI` workflow manually with its pull request
number. The GitHub-hosted job rejects runs initiated by anyone else.

## Operation safety

Every server-mutating operation must remain marked as mutating and must require
`--confirm`. Parsers must fail closed when authentication, CSRF, response shape,
or mutation success cannot be established. Do not add automatic retries for
transactions whose first attempt may already have succeeded.
