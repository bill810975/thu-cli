# Coverage and boundaries

## What is integrated

These repositories are primarily clients and protocol libraries for campus systems. This package does not deploy their backend servers. It ports the relevant client calls into native Rust and exposes them through `thu`, `learn`, and `app` namespaces.

The exact generated operation catalog is `operations.json` and can always be reproduced with `thu-cli services`. Each operation includes its mutation classification and required/optional argument names. This is not a claim that each upstream mobile return model is reproduced byte-for-byte.

- THU Info: academic records, teaching assessment, available classrooms, calendar and custom schedules, course registration/query/drop/PF/priority, library seats/rooms, current sports venue discovery/availability/preflight/booking/records/cancellation, campus card, dorm electricity, income/invoices, news, network devices, campus GitLab, THOS, campus mail, model token/chat, app feedback/announcements, server-side passkey/device management.
- Info application portal: the live 306-card catalogue, category and text
  filtering, native-adapter annotations, per-application metadata and safe SSO
  destination summaries, plus static endpoint discovery across landing pages,
  inline HTML, and JavaScript bundles. Discovery reports read and write
  candidates but never invokes a discovered business endpoint.
- EasyCompute and Paratera: account and cluster metadata, access-key and
  LLM-key metadata, container/image/zone/offering discovery, offline container
  plan construction, price/availability preflight, and guarded machine or key
  creation.
- Classroom reservations: signed SSO, scenes, same-day/cross-day/custom/quick
  availability, records/history, room equipment/groups, workflow reads, and
  guarded lock/book/cancel/extend/end-early writes.
- Campus visitor appointments: temporary visitor menus/forms, plus THOS student
  and relative visitor service limits, service metadata, exact form/process
  identifiers, and read-only Matrix form loading. No inferred THOS submission
  request is exposed.
- Online reimbursement: on-demand Info-to-wsbx SSO, local receipt OCR and
  verification, exact-deduplicated wallet insertion, witness assignment,
  project and item discovery, route-aware electronic-invoice binding,
  encrypted attachments, daily (`052`), smart/foreign-invoice (`051`),
  domestic-travel (`06`), conference (`19`), and international-travel (`1206`)
  draft read/write, international budget and currency-row reads, explicit
  submit, delete, and withdrawal.
- Learn: semesters, student/teacher views, courses, notifications, file categories/downloads/previews, homework details/submission, discussion/question/questionnaire reads, favorites/comments, course order, language, registrar calendar, CourseX metadata sharing.
- App-only: three washer providers, SignalR schedule pairing/transfer, water account/order APIs and update manifest. RFC5545 ICS export is an additional CLI utility.

## Detailed inventory

`thu-public-api-matrix.md` accounts for all 149 explicitly public InfoHelper members in the pinned snapshot, including state fields, aliases and obsolete functions. Counts of class members are not counts of independent microservices. `thu-coverage.md`, `reservations.md`, `card.md`, and `learn-coverage.md` describe implementation details and input/output differences.

## Live verification boundary

Native HTTP requests, forms, encryption, parsers, cookie lifecycles and error
branches have unit/mock coverage. One live account and campus network have also
been tested for direct Info and Learn authentication, persistent session
refresh, Info news reads, Learn semester/course reads and homework reads.
Current sports SSO, scene/area discovery, availability, payment-channel reads,
reservation records and exact-slot booking preflight have also been tested live.
No live sports reservation or payment was created during this verification.

Current classroom SSO, scene/site-type discovery, availability-query contracts,
records, history, recommended/common rooms, equipment, groups, and system
configuration were tested live. EasyCompute/Paratera account exchange, API-key
metadata, container reads/options, and one 5090 price/availability preflight
were tested live. Temporary visitor menus and unread counts, both THOS visitor
service-limit checks, and both Matrix forms were loaded without submission.
No machine, API key, classroom lock/booking/cancellation, or visitor form was
created.

That evidence does not establish every authorization role, enrollment quota,
MFA/CAPTCHA mode, write operation or production response. Untested functionality
remains unverified even where its complete source protocol is ported.

## Partial or intentionally different behavior

- Ordinary username/password and MFA authentication is native. With explicit
  `--trust-device`, the CLI requests a trusted-browser session and can register
  its stable fingerprint after successful interactive 2FA. Browser/platform
  passkey creation, signing and passkey-only login are not implemented.
  Server-side passkey listing/removal/renaming is separate and available.
- CAPTCHA-image challenges are not automatically solved. A user can complete a browser challenge and import their own protected Netscape cookie export.
- Some THU academic/financial/course pages return structured table rows or raw service fields, not the mobile UI's exact localized model. Compare detail documentation before wiring automated downstream calculations.
- Schedule expansion needs explicit semester/first-day context for secondary/summer information. Incomplete context is reported rather than silently claiming a complete timetable.
- Upstream Learn endpoints with a 200-item request cap remain capped; no unsupported pagination protocol was invented.
- MadModel chat uses non-streaming completion and does not execute model tool calls. Mobile voice, React Native screens/widgets, background notifications, local cache/favorites UI and platform attachment sharing are not server microservices and are not reproduced.
- CourseX sharing requires the user's own refresh token. No embedded mobile credentials are copied into the CLI.
- Payments return preparation/QR data; the CLI does not operate a payment wallet or complete payment approval. Water ordering is a separate legacy HTTP operation and can create an obligation.
- The daily-reimbursement (`ywlx=052`) read operations, local PDF receipt
  OCR/verification, wallet insertion, witness assignment, binding, and draft
  project save were verified live. The `051`, `06`, `19`, and `1206` route
  contracts are reconstructed from the current WSBX frontend and covered by
  offline fixtures, but their account entitlements and live response variants
  still need real-account verification. A real approval submission was not
  performed. Mobile camera/QR receipt capture, International Office approvals,
  foreign-exchange/Bank of China processing, remittance, and interactive
  approval routing remain outside this client.
- Local `logout` clears local cookies, not every server session. `learn logout --confirm` invokes Learn's remote logout.
- A failed or timed-out write may already have reached the server. There is no automatic retry of a transaction.
- The EasyCompute landing-page bundle still contains a dead cluster-request
  modal that calls `GET /auth/cluster/info` and `POST /auth/cluster/apply`.
  Those routes return 404 and are not reachable from the live authenticated
  landing page. `easycompute.clusters` instead reads the active Paratera zone
  and cluster metadata used by machine creation.
- Browser verification confirmed that classroom site selection, room
  dictionaries, credit, and lock configuration return 200 when the request
  headers match the browser. A previous cross-origin header set made
  site-selection appear to be a 404 upstream failure.
- The temporary visitor browser first receives 404 from legacy
  `portal/getSecAppInfo`, then recovers through
  `portal/getPortlets?portalUuid=`. The CLI calls the working replacement
  endpoint directly. THOS submission is intentionally not inferred from the
  Matrix form runtime; only form loading and read-only preflight are
  implemented.

## Safe starting point

Begin with `services`, `describe`, `login`, then read-only course/calendar/account queries. Use `--dry-run` before write operations. Keep credentials and real outputs outside this archive. Report an upstream error together with the operation name and redacted shape, not cookies, password, MFA code or complete token-bearing URLs.
