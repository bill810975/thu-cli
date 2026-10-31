# Repository instructions

- Read `README.md` and `docs/LICENSING.md` before changing distribution or
  dependency behavior.
- Use the pinned Rust toolchain and run:

  ```sh
  cargo fmt --all -- --check
  cargo test --locked
  cargo clippy --locked --all-targets -- -D warnings
  ```

- Never add real Cookies, passwords, MFA codes, access tokens, account output,
  personal filesystem paths, or generated session files to fixtures, examples,
  logs, commits, or CI.
- Keep all server-mutating operations marked as mutating and guarded by
  `--confirm`; do not add automatic retries for ambiguous writes.
- Preserve upstream notices under `licenses/`. Do not describe the combined
  repository as entirely MIT while BSL or modified-MIT portions remain.
