# Build and verification report

Last verified: 2026-10-31 on macOS arm64 with Rust
`1.99.1 (58cf8f9c3 2026-10-13)`.

The repository does not require a particular release host. GitHub Actions builds
and tests the source on current Ubuntu. Other architectures should be built on
the target platform or with an explicitly configured cross toolchain.

## Automated checks

```sh
cargo fmt --all -- --check
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
cargo build --locked --release
```

Current result: passed, **61 tests** with zero failures.

- 54 library tests.
- 2 CLI unit tests.
- 5 process-level CLI integration tests.

The tests cover SM2 encryption and wire layout, identity redirects, JSON/HTML/GBK
parsing, AES card responses, reservation and payment-form safeguards, calendar
and timetable parsing, cookie storage, response-shape failures, Learn downloads,
SignalR handshake framing, ICS conversion, authentication refresh detection, and
mutation confirmation guards.

## Live verification

Read-only live checks have been completed for:

- direct Info authentication and persistent session refresh;
- Info news listing, full-text search, and detail retrieval;
- Learn authentication and persistent session refresh;
- Learn semester and course listing;
- Learn homework listing across current courses.
- current sports-venue SSO, scene/area discovery, availability, payment-channel
  reads, reservation records, and exact-slot booking preflight.

No live course selection, homework submission, mail delivery, reservation,
payment, account-security mutation, or other server-side write was performed.

One account and one campus network environment were tested. This does not prove
that every account entitlement, MFA mode, CAPTCHA challenge, service endpoint,
or current production response remains compatible.

## Explicitly not covered

- Real CAPTCHA completion or bypass.
- Native WebAuthn/passkey signing and passkey-only login.
- Every write operation and payment provider.
- Mobile-peer SignalR transfer without a second device.
- All operating systems and CPU architectures.
- Current availability of every service in the generated operation catalog.

Fixtures, mock HTTP tests, and one successful account cannot establish universal
production compatibility. Follow the safety guidance in the README and use
read-only operations first.
