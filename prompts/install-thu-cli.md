# Agent installation prompt

Copy the following prompt into a coding agent that has local shell access.

```text
Install thu-cli from https://github.com/bill810975/thu-cli on this machine.
Work from the repository as the source of truth and do not invent installation
steps when its README or operation metadata already answers a question.

1. Detect the operating system, CPU architecture, shell, and whether `git`,
   `cargo`, `rustc`, and `rustup` are already installed.
2. Clone the repository to `$HOME/.codex/src/thu-cli` unless the user has
   specified a different location. If the directory already exists, verify its
   Git remote points to bill810975/thu-cli and update it instead of overwriting
   local work.
3. Read README.md, rust-toolchain.toml, and docs/LICENSING.md. Respect the
   source-available licensing boundary.
4. Use the repository's pinned Rust toolchain. Then run:
   - `cargo build --locked --release`
   - `cargo test --locked`
   - `cargo fmt --all -- --check`
   - `cargo clippy --locked --all-targets -- -D warnings`
5. Install `target/release/thu-cli` as `$HOME/.local/bin/thu-cli` with mode 0755.
   Do not use sudo unless the user explicitly chose a system-wide prefix.
6. Install `skills/thu-cli` and `skills/thu-reimbursement` under
   `$HOME/.codex/skills/` and validate both with the available Codex skill
   validator.
7. Verify `thu-cli --version`, `thu-cli --help`, `thu-cli services`, and
   `thu-cli describe learn homework`.
8. Do not log in, request the user's password or MFA code, read session files,
   create local credential entries, or execute a mutating campus operation
   during installation.

Report the clone path, binary path, installed skill path, test counts, exact
verification commands, and any blocker. Do not claim success if a build or
verification step failed.
```

The resulting `thu-cli` command is ready for authenticated use, but the user
must complete the first login in their own terminal. See the repository README
and `skills/thu-cli/SKILL.md` for login and operation guidance.
