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

The identity service may stop accepting an unexpired trusted-device Cookie and
show the password form again. For unattended renewal, store the account in the
CLI's local credential JSON beside the selected session profile:

```sh
thu-cli --session "$HOME/.config/thu-cli/info.json" \
  credential set --username USERNAME --confirm
```

The command prompts twice with hidden input. It writes plaintext username/password
entries to `credentials.json` with mode `600`; the CLI refuses symlinks and
broader permissions. Check or remove an entry without printing the password:

```sh
thu-cli --session "$HOME/.config/thu-cli/info.json" \
  credential status --username USERNAME

thu-cli --session "$HOME/.config/thu-cli/info.json" \
  credential delete --username USERNAME --confirm
```

`--refresh` reuses a still-valid trusted session. Ordinary `login` still clears
the current profile to prevent accidental identity mixing; use `--refresh` for
the non-destructive renewal path. If the server still requests a password, the
matching local credential is used automatically when present.

Successful login also writes non-secret refresh metadata beside the private
Cookie profile. Read-only service calls use it to refresh an expired trusted
session automatically and retry once, without a background daemon. State-changing
calls may refresh before execution but are never replayed after an authentication
error because their outcome may already have been committed. Use
`--no-auto-refresh` to disable recovery for one call.

If the server requires CAPTCHA, an untrusted-device MFA challenge, or another
human action, `--non-interactive` fails instead of guessing. Do not extract
Cookies from a browser, put a password in shell history, print the credential
file, or loop authentication attempts. The local file is not protected from
malware, administrators, backups/sync tools, or other processes owned by the
same OS user.

### Renewal Failure Triage

An unexpired local Cookie, including `TSINGHUAUSERID`, does not prove that the
identity server still accepts it. The identity page can contain both the
trusted-device fields and an SM2 password form; the CLI must try the
trusted-device form first and use the local credential only if the server still
requests a password.

When automatic renewal fails, follow this bounded sequence:

1. Check `credential status --username USERNAME` without printing the file.
2. If the user has authorized plaintext local credentials and the account entry
   is missing or stale, run `credential set --username USERNAME --confirm` in a
   real terminal and let the user type the password into its hidden prompts.
   Never ask for the password in chat or pass it as a command-line argument.
3. Retry `login --refresh --trust-device --non-interactive` exactly once.
4. Verify the restored session with a read-only Info call, preferably
   `thu wsbx.user`.
5. If the same password form, MFA, or CAPTCHA still appears, stop and report
   that stage. Do not loop, clear unrelated profiles, switch transports
   blindly, extract Cookies from a browser, or read Keychain.

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

## Course Registration

The current Info "本科生选课" card opens the direct registration host through
`xklogin.do`, then unified identity returns a destination ticket through
`j_acegi_login.do`. This path does not add the legacy CAPTCHA layer. Do not call
`course.captcha` merely because an older client implementation did; use it only
when the live destination explicitly requests that challenge.

Start with read-only calls:

```sh
thu-cli --compact --session "$HOME/.config/thu-cli/info.json" \
  thu course.semesters

thu-cli --compact --session "$HOME/.config/thu-cli/info.json" \
  thu course.selected --args '{"semester":"SEMESTER_ID"}'
```

The registration homepage can default to an upcoming term while the currently
taught term is the preceding semester. Resolve the intended term before
reporting courses. Course selection, deletion, volunteer changes, and P/F
changes are mutations: inspect `course.selected` and `course.queue`, use the
live `semester`, course number and section, and require explicit authorization
for that exact action. Never infer a section number from a course name.

## Sports Venue Booking

The retired `50.tsinghua.edu.cn/gymbook` service is not used. Current sports
reservations run through `https://www.sports.tsinghua.edu.cn/venue/` with an
on-demand SSO token exchange. The platform token remains in the private
mode-600 `sports-auth` sidecar; never print or copy it.

Start with reads:

```sh
thu-cli --compact --session "$HOME/.config/thu-cli/info.json" \
  thu sports.auth-status

thu-cli --compact --session "$HOME/.config/thu-cli/info.json" \
  thu sports.scenes --args '{"query":"网球"}'

thu-cli --compact --session "$HOME/.config/thu-cli/info.json" \
  thu sports.availability \
  --args '{"scene_uuid":"SCENE_UUID","date":"2026-11-12"}'
```

`sports.availability` defaults to bookable sessions only and returns normalized
court/session UUIDs, start/end times, price, capacity, and payment metadata.
Use `sports.areas` and `sports.site-types` when a scene has multiple buildings,
rooms, device kinds, or site kinds. Availability can be scoped with
`class_type_uuid`, `class_type_enum`, `dev_kind_uuid`, or `site_kind_id`.

Before any reservation, run `sports.preflight` for the exact site and time:

```sh
thu-cli --compact --session "$HOME/.config/thu-cli/info.json" \
  thu sports.preflight \
  --args '{"scene_uuid":"SCENE_UUID","site_uuid":"SITE_UUID","start":"2026-11-12 18:00","end":"2026-11-12 19:00","pay_type":"PAY_ONLINE"}'
```

Review the returned court, location, date/time, price, payment mode, form type,
approval needs, and captcha requirement with the user. Do not guess a court,
time, member list, approver, form value, or payment mode. After the user
explicitly confirms those exact details, change `sports.preflight` to
`sports.book` and add `--confirm`.

`sports.book` revalidates that both the site and exact session are currently
bookable, obtains a short lock, creates one reservation, and checks the order.
It never completes payment and never retries an ambiguous write. If the result
is unclear, inspect `sports.records` before retrying. Continue payment only on
the official venue/order page and only within the displayed hold deadline.
Cancellation requires the exact reservation UUID from `sports.records` and an
explicit user request.

## Online Reimbursement

The `wsbx.*` operations use the authenticated Info profile to acquire a wsbx SSO
ticket on demand. They keep the rotating wsbx token in a private mode-600
sidecar; never print or copy it. No background service is required.

For receipt classification and the domestic-versus-foreign document workflow,
use the repository's `thu-reimbursement` skill when it is installed.

Start with reads:

```sh
thu-cli --compact --session "$HOME/.config/thu-cli/info.json" \
  thu wsbx.business-types

thu-cli --compact --session "$HOME/.config/thu-cli/info.json" \
  thu wsbx.project.list --args '{"page_size":10}'

thu-cli --compact --session "$HOME/.config/thu-cli/info.json" \
  thu wsbx.draft.list --args '{"status":"ALL-2"}'
```

Use `thu wsbx.routes` to inspect the supported route map. Pass `ywlx` explicitly
for new work:

| `ywlx` | Workflow |
| --- | --- |
| `052` | Daily reimbursement |
| `051` | Smart reimbursement; normally used for a foreign invoice/receipt already paid personally |
| `06` | Domestic travel |
| `19` | Conference |
| `1206` | International travel with approved RMB/foreign-currency rows |

For a normal domestic e-invoice on daily route `052`, the CLI defaults
`no_paper` and `is_online` to true. Preserve those defaults for paperless online
push unless the user explicitly requests and Finance permits another route.

The normal sequence is: ingest and verify a source invoice in the receipt wallet,
allocate a draft number, bind a project, read the live route template, bind the
unused wallet invoice, upload evidence, allocate payment, save, inspect, then
submit separately.
`invoice.add`, `invoice.add-witness`, `draft.bind-project`, `invoice.bind`,
`attachment.upload`, `draft.save`, and `draft.submit` mutate server state.
`draft.template`, `draft.get`, and invoice unbinding dispatch by route rather
than assuming `052`.

Daily `052` submission requires allocated payment rows totaling the
reimbursement amount. For an employee-paid expense, pass
`"self_payment": true`; if several bank accounts exist, use
`"self_payment":{"account":"gzkh1","note":"报销款"}`. The CLI reads the
employee's protected account data, creates a method-`08` campus private-payment
row, and refuses a zero or mismatched allocation before submission.
`wsbx.payment.init` verifies saved rows and `wsbx.payment.methods` lists enabled
methods.

Use CLI receipt ingestion for a downloaded PDF, OFD, image, or supported ZIP:

```sh
thu-cli --compact --session "$HOME/.config/thu-cli/info.json" \
  thu wsbx.invoice.identify \
  --args '{"file":"/absolute/path/to/receipt.pdf"}'

thu-cli --compact --session "$HOME/.config/thu-cli/info.json" \
  thu wsbx.invoice.add \
  --args '{"file":"/absolute/path/to/receipt.pdf"}' --confirm

thu-cli --compact --session "$HOME/.config/thu-cli/info.json" \
  thu wsbx.invoice.wallet.list \
  --args '{"query":"INVOICE_NUMBER","status":"pending"}'
```

`invoice.identify` is read-only on WSBX. `invoice.add` performs OCR,
verification, exact invoice-number deduplication, and wallet insertion. Do not
repeat `invoice.add` after an ambiguous timeout; inspect `invoice.wallet.list`
and both pending/used states first. If binding fails because the same person is
handler, project owner, and invoice handler, use `wsbx.employee.search`, then
authorized `wsbx.invoice.add-witness` with an accurate note before retrying bind.

Build and validate the complete route payload offline before a write:

```sh
thu-cli --compact thu wsbx.draft.preview \
  --args-file PRIVATE_REIMBURSEMENT_ARGS.json
```

For complex drafts, `wsbx.draft.save` and `wsbx.draft.submit` accept a
`payload` object with the exact server-shaped route fields. The builder requires
`payload.ywbh` to match the top-level `ywbh` and forces the correct `saveType`.

`wsbx.invoice.wallet.list` reads `我的票夹`; `wsbx.invoice.list` reads the
electronic-invoice pool. The latter defaults to pending invoices. Use
`--args '{"query":"INVOICE_NUMBER","status":"used"}'` to check invoices already
used by a reimbursement in progress or completed. An empty pending result alone
does not identify which reimbursement consumed the invoice.

Before `wsbx.draft.submit`, inspect both `wsbx.draft.get` and
`wsbx.draft.status`. Submit only when the user explicitly authorized the exact
receipt and fields. Never retry an ambiguous submission; inspect status first.
See the repository's `docs/wsbx.md` for argument structure.

The daily `052` submission is two-phase in WSBX. `zt=8` means **pending
approval submission**, not submitted. `draft.submit` now performs both phases;
it reads or generates the approval workflow, runs `approval.check`, and then
calls `approval.submit`. Treat only `zt=9` as entered approval. An existing
`zt=8` receipt can be finalized by repeating `draft.submit` with `ywbh`, or by
using `wsbx.approval.workflow`, `wsbx.approval.check`, and authorized
`wsbx.approval.submit`.

International-travel work starts from `wsbx.intl.budget` and uses
`wsbx.intl.get`, `wsbx.intl.currency.split`, `wsbx.intl.invoices`,
`wsbx.intl.invoice.update`, and `wsbx.intl.files` as needed. WSBX draft support
does not complete external International Office approval, foreign-exchange
pre-review, bank exchange, or remittance.

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
