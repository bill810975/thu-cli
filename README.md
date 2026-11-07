# thu-cli

[English](README.md) | [中文](README.zh-CN.md)

Native Rust command-line clients for authenticated Tsinghua campus and Learn
services.

The CLI implements login, SM2 password encryption, Cookie persistence, SSO
roaming, JSON/HTML/table parsing, and service-specific safety checks in Rust.
It does not require Node.js, Python, or the mobile apps at runtime.

The implementation and compatibility work build on two upstream projects:

- [THU Info](https://github.com/thu-info-community/thu-info-app), particularly
  `thu-info-lib`, for campus-service protocols and behavior.
- [learnX](https://github.com/robertying/learnX), together with its
  `thu-learn-lib` dependency, for Learn protocol coverage and app workflows.

These are source references, not endorsements by the upstream projects.

> **License boundary:** this repository is source-available, not an OSI
> open-source distribution at this time. Portions are derived from THU Info
> under the Business Source License 1.1 and from learnX under its modified MIT
> terms. Read [docs/LICENSING.md](docs/LICENSING.md) and the notices in
> [`licenses/`](licenses/) before redistribution or production use.

## Quick Start
For an agent-driven installation, give an agent the prompt in
[prompts/install-thu-cli.md](prompts/install-thu-cli.md).

## Features

- Direct campus and WebVPN-aware Tsinghua Info authentication.
- Separate persistent profiles for Info and Learn.
- Browser-like trusted login through `--trust-device`.
- On-demand trusted-session refresh for read-only calls, with no background
  daemon required.
- Hidden password input, SM2 encryption, MFA, and an optional mode-600 local
  JSON credential for unattended renewal.
- Campus news search, courses, homework, schedules, calendar, library, current
  sports-venue discovery/booking, card, network, mail, invoices, THOS, GitLab,
  and other service operations.
- Reimbursement drafts for daily (`052`), smart/foreign-invoice (`051`),
  domestic travel (`06`), conference (`19`), and international travel (`1206`),
  including local receipt OCR/verification and wallet insertion,
  same-person witness assignment, route-specific templates, invoices,
  attachments, payments, international budgets, RMB/foreign-currency rows,
  save, submit, delete, and withdrawal.
- Learn courses, notifications, files, homework, questionnaires, and downloads.
- JSON output, operation discovery, explicit mutation confirmation, and
  non-overwriting downloads.
- A reusable Codex skill and a copy-paste agent installation prompt.

Live read-only Info and Learn authentication and session refresh have been
tested. Write operations, account-specific entitlements, CAPTCHA flows, and
every campus service remain subject to the boundaries in
[docs/COVERAGE.md](docs/COVERAGE.md).

The current `sports.tsinghua.edu.cn` venue system has live-tested SSO, scene,
availability, payment-channel, record, and booking-preflight reads. Booking
creation remains a separately confirmed mutation and never completes payment.

The daily reimbursement client has also been checked against a live account
through local receipt OCR/verification, wallet insertion, witness assignment,
invoice binding, project binding, and draft save. The added smart/foreign-invoice,
domestic-travel, conference, and international-travel contracts are
reconstructed from the current WSBX frontend and covered by offline fixtures;
they still need live account and entitlement verification.

## Requirements

- Git
- Rustup
- A C toolchain supported by the Rust target
- A Tsinghua account for authenticated commands
- Campus network access or WebVPN for restricted services

The repository pins Rust in [`rust-toolchain.toml`](rust-toolchain.toml).
Install Rustup from <https://rustup.rs/> if it is not already available.

## Clone

```sh
git clone https://github.com/bill810975/thu-cli.git
cd thu-cli
```

## Build and test

Rustup will install the pinned toolchain when the project is first built.

```sh
cargo build --locked --release
cargo test --locked
```

Recommended release checks:

```sh
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
```

The release binary is written to `target/release/thu-cli`.

## Install

Prebuilt archives for Linux, macOS, and Windows are available on the
[releases page](https://github.com/bill810975/thu-cli/releases). Download the
archive for your platform, verify it against `SHA256SUMS`, unpack it, and put
`thu-cli` (or `thu-cli.exe` on Windows) on `PATH`.

Release targets:

- macOS universal (`x86_64` and Apple Silicon)
- Windows `x86_64`
- Linux `x86_64`; the same binary can run in WSL

### Install from the clone

```sh
cargo install --path . --locked
thu-cli --version
```

### Install a local release build

```sh
mkdir -p "$HOME/.local/bin"
install -m 0755 target/release/thu-cli "$HOME/.local/bin/thu-cli"
```

Ensure `$HOME/.local/bin` is in `PATH`.

## First login

Use separate profiles for Info and Learn. Passwords are requested in the
terminal and are not accepted as command-line arguments.

```sh
thu-cli --session "$HOME/.config/thu-cli/info.json" \
  login --username YOUR_STUDENT_ID --service info --trust-device

thu-cli --session "$HOME/.config/thu-cli/learn.json" \
  login --username YOUR_STUDENT_ID --service learn --trust-device
```

`--trust-device` requests a persistent trusted-browser session. After a
successful login, ordinary session refresh can run without another password:

```sh
thu-cli --session "$HOME/.config/thu-cli/info.json" \
  login --username YOUR_STUDENT_ID --service info \
  --refresh --trust-device --non-interactive

thu-cli --session "$HOME/.config/thu-cli/learn.json" \
  login --username YOUR_STUDENT_ID --service learn \
  --refresh --trust-device --non-interactive
```

The CLI stores non-secret refresh metadata beside each Cookie profile and uses
it automatically when a read-only call finds an expired session. State-changing
calls are never replayed after their request may have reached the server. Use
`--no-auto-refresh` to disable this recovery for one call.

If the identity service stops accepting the trusted-device Cookie, store the
password in the CLI's private local JSON credential file:

```sh
thu-cli --session "$HOME/.config/thu-cli/info.json" \
  credential set --username YOUR_STUDENT_ID --confirm

thu-cli --session "$HOME/.config/thu-cli/info.json" \
  credential status --username YOUR_STUDENT_ID
```

The password is stored in plaintext in `credentials.json` beside the selected
session profile, with file mode `600`; the CLI refuses symlinks and broader
permissions. Read-only operations use the matching account entry automatically
when the server requests password reauthentication. The password is still
encrypted with SM2 for network transport and is never printed. This file is not
protected against malware, an administrator, backup/sync software, or another
process running as the same OS user. Use `credential delete ... --confirm` to
remove an account entry. Info and Learn profiles in the same directory share the
credential file, while separate usernames are stored as separate entries.

See [docs/auth.md](docs/auth.md) for session lifecycle, MFA, WebVPN, and
credential boundaries.

## Discover and run operations

The live catalog is authoritative:

```sh
thu-cli services
thu-cli services thu
thu-cli services learn
thu-cli services app
thu-cli describe learn homework
```

Examples:

```sh
thu-cli --compact --session "$HOME/.config/thu-cli/info.json" \
  thu news.list --no-roam --args '{"page":1,"length":10}'

thu-cli --compact --session "$HOME/.config/thu-cli/info.json" \
  thu news.search --no-roam \
  --args '{"keyword":"任免","page":1,"exact":false}'

thu-cli --compact --session "$HOME/.config/thu-cli/learn.json" \
  learn semesters
```

Sports venue discovery and booking preflight:

```sh
thu-cli --compact --session "$HOME/.config/thu-cli/info.json" \
  thu sports.scenes --args '{"query":"网球"}'

thu-cli --compact --session "$HOME/.config/thu-cli/info.json" \
  thu sports.availability \
  --args '{"scene_uuid":"SCENE_UUID","date":"2026-11-12"}'

thu-cli --compact --session "$HOME/.config/thu-cli/info.json" \
  thu sports.preflight \
  --args '{"scene_uuid":"SCENE_UUID","site_uuid":"SITE_UUID","start":"2026-11-12 18:00","end":"2026-11-12 19:00","pay_type":"PAY_ONLINE"}'
```

Repeat `sports.preflight` with `--confirm` as `sports.book` only after explicit
confirmation of the exact court, date, time, and payment mode. See
[docs/reservations.md](docs/reservations.md).

Daily reimbursement discovery and a safe read-only start:

```sh
thu-cli --compact --session "$HOME/.config/thu-cli/info.json" \
  thu wsbx.business-types

thu-cli --compact --session "$HOME/.config/thu-cli/info.json" \
  thu wsbx.project.list --args '{"page_size":10}'

thu-cli --compact --session "$HOME/.config/thu-cli/info.json" \
  thu wsbx.draft.list --args '{"status":"ALL-2"}'
```

The SSO ticket and rotating reimbursement token are obtained on demand from the
already-authenticated Info profile. The token is stored in a mode-`600` private
sidecar and is never printed. See [docs/wsbx.md](docs/wsbx.md) for the complete
receipt-wallet, draft, invoice, attachment, save and explicit-submit workflow.

## Safety model

- Read-only operations do not require `--confirm`.
- Server-mutating operations require `--confirm`.
- `--dry-run` validates operation metadata without sending a network request.
- A timed-out mutation may already have succeeded; inspect server state before
  retrying.
- Downloads and exports refuse to overwrite existing files.
- Cookie files and password files require file mode `600`.
- Do not commit sessions, Cookies, passwords, MFA codes, tokens, or real API
  output.

Do not use this tool to bypass CAPTCHA, MFA, passkeys, access controls, or
institutional policy.

## Codex skill

The repository includes two installable Codex skills:

```sh
mkdir -p "$HOME/.codex/skills"
cp -R skills/thu-cli "$HOME/.codex/skills/"
cp -R skills/thu-reimbursement "$HOME/.codex/skills/"
```

The skill covers operation discovery, Info and Learn workflows, session
refresh, mutation safety, and secret handling. The reimbursement skill routes
domestic invoices and foreign `INVOICE`/`RECEIPT` cases through the correct
evidence, approval, foreign-exchange, and CLI paths.

For an agent-driven installation, give an agent the prompt in
[prompts/install-thu-cli.md](prompts/install-thu-cli.md).

## Repository layout

```text
src/                 Rust implementation
tests/               process-level CLI tests
docs/                protocol, coverage, testing, and licensing notes
licenses/            upstream and dependency notices
examples/            example inputs
skills/thu-cli/      reusable Codex skill
skills/thu-reimbursement/  domestic and foreign reimbursement workflow
prompts/             copy-paste agent prompts
```

## Development

```sh
cargo fmt --all
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
```

Before submitting a change, confirm that it does not add secrets, local paths,
real account data, or incompatible third-party code. See
[CONTRIBUTING.md](CONTRIBUTING.md) and [SECURITY.md](SECURITY.md).

## Licensing

The original standalone Rust glue is offered under the MIT text in
[`licenses/original-glue-MIT.txt`](licenses/original-glue-MIT.txt), without
removing rights and restrictions that apply to derived portions.

- THU Info-derived portions remain subject to BSL 1.1 with Additional Use Grant
  `None` until the upstream Change Date.
- learnX notices include an additional institutional-use restriction.
- thu-learn-lib is MIT.
- Rust dependencies and the Rust standard library retain their own licenses in
  [`licenses/`](licenses/).

The root [LICENSE](LICENSE) is a combined-distribution notice, not a claim that
the whole repository is MIT. This repository does not provide legal advice.
