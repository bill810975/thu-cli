# THU Info native Rust port

Source pinned: `thu-info-community/thu-info-app` commit `7930cad3f0d7243ad54df3ce4443b911113eef91`.
The protocols are derived from `packages/thu-info-lib/src`. The upstream package and repository are **Business Source License 1.1**, not MIT at this revision. Keep the included license and notices. This package is not a grant of production-use permission.

## What this version implements

The CLI catalogue is generated from the actual operation registry. Run:

```
thu-cli services thu
thu-cli describe thu course.select
```

Domains: academic course registration; primary, secondary and summer CR timetables; personal calendars; unified identity profile/devices/passkey management; academic grades; course assessment; classrooms; invoices; bank and graduate-income history; degree programme tables; dorm electricity; network self-service; campus news and subscriptions; library seats/rooms; sports reservations; campus card; THOS tasks/services; campus GitLab; Coremail sending; THU Info app feedback/version endpoints; campus MadModel chat.

`docs/thu-public-api-matrix.md` inventories **every public InfoHelper member**. An operation mapping does not mean identical mobile presentation, identical output schema, or a live-verified server integration. Auth/local utility/obsolete methods are classified separately. Passkey key generation and enrollment are not implemented.

## Linux use

A real session is required. Use native login and target SSO, or import your own valid Netscape cookie export. Do not share credentials or the session file. Some campus endpoints require campus networking or WebVPN. MFA/CAPTCHA can require manual interaction. `--username` on a call supplies destination reauthentication through a hidden password prompt; `--no-roam` is only for an already-valid destination session. `thu session.logout-remote --confirm` requests WebVPN server logout; `logout --confirm` clears local cookies. These are separate operations and do not revoke all existing third-party target sessions.

```
thu-cli login --username YOUR_ID --service info
thu-cli thu course.semesters --username YOUR_ID
thu-cli thu course.search --username YOUR_ID --args '{"semester":"2026-2027-1","name":"编译原理","page":1}'
thu-cli thu course.selected --username YOUR_ID --args '{"semester":"2026-2027-1"}'
```

All state-changing operations require the outer CLI `--confirm`. A dry run checks operation and required arguments, but does not prove account access or simulate server outcomes. Selection windows, eligibility, quotas, charges and CAPTCHA remain server-controlled.

```
thu-cli thu course.select --args '{"semester":"2026-2027-1","id":"COURSE_ID","seq":"0","priority":"rx","will":1}' --dry-run
# Only after reviewing the exact course:
thu-cli thu course.select --username YOUR_ID --args '{"semester":"2026-2027-1","id":"COURSE_ID","seq":"0","priority":"rx","will":1}' --confirm
```

The example course ID is deliberately a placeholder. It is not a recommendation to enroll. Mutations are never retried automatically. A transport failure after a request can leave an uncertain outcome; check selected courses, reservations, sent mail, or transaction history before another attempt.

### Calendar

Dates are Beijing civil dates. Primary requests are batched in at most 21-day windows. The result is a flat array of individual events, not the mobile app's merged lesson objects.

```
thu-cli thu calendar.semesters --username YOUR_ID
thu-cli thu schedule.list --username YOUR_ID --args '{"start":"2026-11-01","end":"2026-11-30","first_day":"2026-09-07","semester":"2026-2027-1","weeks":16}'
thu-cli thu schedule.create --username YOUR_ID --args '{"name":"Meeting","location":"Room 109","date":"2026-11-12","start":"10:30","end":"12:00"}' --confirm
```

Use the actual semester first day and week count from `calendar.semesters`; the above date is an illustrative value. `first_day` is needed for secondary timetable expansion. `semester` is needed to determine whether summer CR fallback is required. Missing these inputs produces explicit warnings and `complete: false`, rather than claiming a complete timetable. A semester ending in `-3` triggers the CR timetable fallback; that may require a valid CR session. Individual calendar changes report submission and require refresh to verify persistence.

### Response differences and bounded limits

- Most JSON APIs retain server keys and pagination. The CLI does not silently claim every page was fetched. Request `page`/`size` as documented in the catalogue. THOS returns validated raw page objects, including phase steps when present. Some source adapters fetched all pages automatically; this version exposes explicit pagination.
- Course search and selected-course responses normalize table columns. Priority statistics, degree programmes, CR `course.schedule`, bank payments and parts of assessment retain structured table/control data rather than every mobile computed field. `schedule.list` performs CR event expansion for summer.
- Grade reports return server grades and grade points. Mobile-only GPA recalculation and required-course filtering are not yet replicated.
- News detail accepts an `id` or a Tsinghua `url`. New JSON detail and PDF bodies are supported; legacy pages retain full HTML and text instead of reproducing each site's mobile-content selector. PDF data uses base64.
- Assessment form controls expose names/values; submission requires explicit user-provided `fields`. The CLI does not auto-score teachers or generate assessment answers.
- `income.bank` accepts a `years` array of strings or integers. The mobile convenience function that enumerates every available year is not reproduced automatically.
- `dorm.score` returns the chart URL, not an inline rendered image.
- Library and sports semantics/limits are detailed in `reservations.md`; card semantics are in `card.md`.
- Payment QR initiation creates a payment request but does **not** complete a third-party payment. A bank-card recharge is a real financial operation when a user explicitly executes that command. No financial operation was executed while building or testing.
- `model.chat` uses the upstream campus chat endpoint with non-streaming output. Tool calls are returned as data; they are never executed. App conversation history, RAG orchestration and notification UI are not recreated. This call sends the supplied messages to the campus model service and is guarded as a mutation.
- Telemetry endpoints exist as explicit `app.startup-stat` and `app.usage-stat` operations. The CLI never sends telemetry automatically.
- `passkey.list/remove/rename` and trusted-device removal operate on server registrations. This is not native passkey authentication or enrollment. The broad `device.forget-thu-info` operation follows an upstream prefix-based cleanup and can remove multiple existing THU Info APP device registrations; prefer an exact `device.remove` UUID when possible.

## Verification

No university credentials were available, and no real account login, enrollment, booking, mail send, credential change or payment was performed. A successful build is **not** a promise that all live campus systems still accept these protocols.

Offline tests cover GBK percent encoding, authentic form-token extraction, pagination parsing, JSONP rejection, semester alignment, secondary timetable expansion, mutation catalogue guards summer CR timetable expansion, payment-destination validation, and an actual local mock HTTP form request. Shared auth, reservations/card, learning and app tests are documented in the package test report.
