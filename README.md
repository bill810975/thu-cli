# thu-cli

Native Rust command-line clients for authenticated Tsinghua campus and Learn
services.

The CLI implements login, SM2 password encryption, Cookie persistence, SSO
roaming, JSON/HTML/table parsing, and service-specific safety checks in Rust.
It does not require Node.js, Python, or the mobile apps at runtime.

> **License boundary:** this repository is source-available, not an OSI
> open-source distribution at this time. Portions are derived from THU Info
> under the Business Source License 1.1 and from learnX under its modified MIT
> terms. Read [docs/LICENSING.md](docs/LICENSING.md) and the notices in
> [`licenses/`](licenses/) before redistribution or production use.

## Features

- Direct campus and WebVPN-aware Tsinghua Info authentication.
- Separate persistent profiles for Info and Learn.
- Browser-like trusted login through `--trust-device`.
- Hidden password input, SM2 encryption, MFA, and optional Keychain reuse.
- Campus news search, courses, homework, schedules, calendar, library, sports,
  card, network, mail, invoices, THOS, GitLab, and other service operations.
- Learn courses, notifications, files, homework, questionnaires, and downloads.
- JSON output, operation discovery, explicit mutation confirmation, and
  non-overwriting downloads.
- A reusable Codex skill and a copy-paste agent installation prompt.

Live read-only Info and Learn authentication and session refresh have been
tested. Write operations, account-specific entitlements, CAPTCHA flows, and
every campus service remain subject to the boundaries in
[docs/COVERAGE.md](docs/COVERAGE.md).

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

On macOS, an optional Keychain-backed password can be stored for the case where
the persistent identity Cookie expires:

```sh
thu-cli credential set --username YOUR_STUDENT_ID --confirm
thu-cli credential status --username YOUR_STUDENT_ID
```

Keychain reuse is optional. If a Keychain is explicitly used from an SSH
session and is locked, unlock it through a terminal prompt with
`security unlock-keychain "$HOME/Library/Keychains/login.keychain-db"`. Do not
open a GUI terminal solely for this CLI.

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

The repository includes an installable Codex skill:

```sh
mkdir -p "$HOME/.codex/skills"
cp -R skills/thu-cli "$HOME/.codex/skills/"
```

The skill covers operation discovery, Info and Learn workflows, session
refresh, mutation safety, and secret handling.

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
