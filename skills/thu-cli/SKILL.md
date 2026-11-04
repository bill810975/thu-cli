---
name: thu-cli
description: Use the locally installed thu-cli for authenticated Tsinghua Info and Learn queries, including news search, courses, homework, and guarded campus-service actions. Do not use it for generic web browsing.
---

# Tsinghua CLI

Use the installed `thu-cli` binary (normally `$HOME/.local/bin/thu-cli`) instead of reproducing Tsinghua login,
Cookie, CSRF, or endpoint behavior by hand. Treat every result as private
account data and do not reveal session files, Cookie values, passwords, MFA
codes, tokens, or unrelated records.

## Discover Before Calling

Treat the live operation catalog as authoritative:

```sh
thu-cli services thu
thu-cli services learn
thu-cli services app
thu-cli describe SERVICE OPERATION
```

The CLI returns JSON. Use `--compact` when piping to tools. Every operation
declares `mutating`, `required`, and `optional` arguments; do not guess argument
names when `describe` can answer the question.

## Session Profiles

Use separate private Cookie stores:

```text
Info:  $HOME/.config/thu-cli/info.json
Learn: $HOME/.config/thu-cli/learn.json
```

Pass the relevant file with the global `--session` option. Session files must
have mode 600. `thu-cli status` only reports unexpired local Cookies; it does
not prove that the server still accepts them.

## Authentication

On campus, Info normally uses direct campus access:

```sh
thu-cli --session "$HOME/.config/thu-cli/info.json" \
  login --username USERNAME --service info --trust-device
```

Use `--webvpn` for Info only when direct campus access is unavailable. Learn is
an independent session:

```sh
thu-cli --session "$HOME/.config/thu-cli/learn.json" \
  login --username USERNAME --service learn \
  --two-factor-method mobile --trust-device
```

`--trust-device` asks unified identity to trust this CLI device after a
successful challenge and requests the persistent `singleLogin` browser session.
The server may then issue `TSINGHUAUSERID`, a persistent unified-identity Cookie.
This is the preferred browser-like renewal mechanism and does not require a GUI.

After one interactive login with `--refresh --trust-device`, refresh both
profiles non-interactively:

```sh
thu-cli --session "$HOME/.config/thu-cli/info.json" \
  login --username USERNAME --service info \
  --refresh --trust-device --non-interactive

thu-cli --session "$HOME/.config/thu-cli/learn.json" \
  login --username USERNAME --service learn \
  --refresh --trust-device --non-interactive
```

Do not add `--keychain` while this succeeds. The Keychain is only an optional
fallback for the case where the persistent identity Cookie has expired and the
user explicitly wants unattended password reauthentication. Store a fallback
password in Keychain only after that need is observed:

```sh
thu-cli credential set --username USERNAME --confirm
```

The command prompts twice through macOS `security`; the password does not enter
shell history or command arguments. Check and remove it without printing it:

```sh
thu-cli credential status --username USERNAME
thu-cli credential delete --username USERNAME --confirm
```

Refresh an existing profile without clearing its Cookies:

```sh
thu-cli --session "$HOME/.config/thu-cli/info.json" \
  login --username USERNAME --service info \
  --refresh --trust-device --non-interactive

thu-cli --session "$HOME/.config/thu-cli/learn.json" \
  login --username USERNAME --service learn \
  --refresh --trust-device --non-interactive
```

`--refresh` reuses a still-valid trusted session. Ordinary `login` still clears
the current profile to prevent accidental identity mixing; use `--refresh` for
the non-destructive renewal path. Add `--keychain` only when persistent Cookie
renewal has failed and a stored password is the chosen fallback.

Successful login also writes non-secret refresh metadata beside the private
Cookie profile. Read-only service calls use it to refresh an expired trusted
session automatically and retry once, without a background daemon. State-changing
calls may refresh before execution but are never replayed after an authentication
error because their outcome may already have been committed. Use
`--no-auto-refresh` to disable recovery for one call.

If the server requires CAPTCHA, an untrusted-device MFA challenge, or another
human action, `--non-interactive` fails instead of guessing. Do not extract
Cookies from a browser, put a password in shell history, expose Keychain data,
or loop authentication attempts.

Do not open a GUI terminal merely to populate Keychain from an SSH workflow. If
Keychain is explicitly chosen as the fallback and the keychain is locked, run
`security unlock-keychain ~/Library/Keychains/login.keychain-db` in the terminal
first; its prompt is terminal-based.

## Info Workflows

For the current direct Info session, use `--no-roam` with news operations:

```sh
thu-cli --compact --session "$HOME/.config/thu-cli/info.json" \
  thu news.list --no-roam --args '{"page":1,"length":20}'

thu-cli --compact --session "$HOME/.config/thu-cli/info.json" \
  thu news.search --no-roam \
  --args '{"keyword":"任免","page":1,"exact":false}'

thu-cli --compact --session "$HOME/.config/thu-cli/info.json" \
  thu news.detail --no-roam --args '{"id":"NEWS_ID"}'
```

`news.list` returns items under `object.dataList`; full-text search returns
items under `object.resultsList`. Search can return the same document through
multiple publication channels, so deduplicate by title, time, and document ID.

For personnel-change research, search terms such as `任免`, `任前公示`, `聘任`,
`人事`, `职称`, and `岗位调整`. Check the publication date and open
`news.detail` before stating a personnel change. Do not infer an appointment or
removal from a title alone.

For other campus services, inspect the catalog first. Prefixes currently cover
`course`, `schedule`, `calendar`, `info`, `news`, `library`, `sports`, `card`,
`network`, `program`, `assessment`, `classroom`, `dorm`, `invoice`, `thos`, and
`gitlab`, plus the `wsbx` reimbursement workflow.

## Online Reimbursement

The `wsbx.*` operations use the authenticated Info profile to acquire a wsbx SSO
ticket on demand. They keep the rotating wsbx token in a private mode-600
sidecar; never print or copy it. No background service is required.

Start with reads:

```sh
thu-cli --compact --session "$HOME/.config/thu-cli/info.json" \
  thu wsbx.business-types

thu-cli --compact --session "$HOME/.config/thu-cli/info.json" \
  thu wsbx.project.list --args '{"page_size":10}'

thu-cli --compact --session "$HOME/.config/thu-cli/info.json" \
  thu wsbx.draft.list --args '{"status":"ALL-2"}'
```

The daily reimbursement workflow is `ywlx=052`: allocate a draft number, bind a
project, use its expense-item template, bind unused invoices and attachments,
save, then submit separately. `draft.bind-project`, `invoice.bind`,
`attachment.upload`, `draft.save`, and `draft.submit` all mutate server state.

`wsbx.invoice.list` defaults to pending invoices. Use
`--args '{"query":"INVOICE_NUMBER","status":"used"}'` to check invoices already
used by a reimbursement in progress or completed. An empty pending result alone
does not identify which reimbursement consumed the invoice.

Before `wsbx.draft.submit`, inspect both `wsbx.draft.get` and
`wsbx.draft.status`. Submit only when the user explicitly authorized the exact
receipt and fields. Never retry an ambiguous submission; inspect status first.
See the repository's `docs/wsbx.md` for argument structure.

## Learn Workflows

Start from the authenticated Learn session:

```sh
thu-cli --compact --session "$HOME/.config/thu-cli/learn.json" \
  learn semesters

thu-cli --compact --session "$HOME/.config/thu-cli/learn.json" \
  learn current-semester
```

Find the current semester ID, then list courses:

```sh
thu-cli --compact --session "$HOME/.config/thu-cli/learn.json" \
  learn courses --args '{"semester":"SEMESTER_ID"}'
```

Read homework for every course ID, not just the first course:

```sh
thu-cli --compact --session "$HOME/.config/thu-cli/learn.json" \
  learn homework --args '{"course":"COURSE_ID"}'
```

Homework in the unsubmitted endpoint has `"submitted": false`. Report its title,
course, deadline, and late-submission deadline; sort actionable items by the
nearest deadline. Include only work that is actually within the relevant
semester or assignment window, and distinguish a missing deadline from a
platform parsing failure.

## Mutation Safety

Read-only operations may be used to answer the user's request. A `mutating:
true` operation changes campus data and requires explicit authorization for the
specific action.

Before an authorized mutation:

1. Run `thu-cli describe` and confirm required arguments.
2. Run with `--dry-run` when supported to validate offline.
3. Use `--confirm` only for the exact operation the user authorized.
4. If the outcome is ambiguous, inspect state before retrying.

Never submit or alter homework, select courses, send mail, create or cancel
bookings, pay or recharge, change account security, or modify trusted devices
without a direct instruction. Never automatically retry a mutation whose
outcome may already have taken effect.

For sensitive arguments, prefer `--args-file` with a private temporary file,
then remove it. Do not put passwords, tokens, or other secrets in `--args`.
